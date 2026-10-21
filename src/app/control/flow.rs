mod feed;
mod manage;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use super::{Handled, NO_PROJECT, Pending, Stage, json, none, sized};
use crate::app::App;
use crate::app::flow_runs::{Made, RunTrees, Starting};
use crate::control::contract::Refusal;
use crate::control::{self, BrokenFlow, Done, Exported, FlowInfo, FlowSpec, Flows, Imported, RoleInfo, RunInfo};
use crate::error;
use crate::flows::dialog::project_risks;
use crate::flows::share::{self, To};
use crate::flows::store::{self, Listing, Origin, Saved};
use crate::flows::{self, Flow, Role, builtin};
use crate::git;
use crate::git::worktree;
use crate::log;
use crate::paths;
use crate::project::Hierarchy;

impl App {
    pub(super) fn flow_list_request(&self, caller: Option<u64>, list: &control::FlowList) -> Handled {
        let p = match list.project {
            Some(id) => Some(self.projects.project_index(id).ok_or_else(|| none("project", id))?),
            None => self.here(caller).project,
        };
        let listing = self.flow_listing(p.map(|p| self.projects[p].path.as_path()));
        let (flows, hidden) = manage::info_listing(&listing);
        let builtins = listing.builtins;
        let broken = listing.broken.into_iter().map(|(path, error)| BrokenFlow { path, error }).collect();
        Ok(Some(json(&Flows { flows, broken, hidden, builtins })))
    }

    pub(super) fn flow_export_request(&self, caller: Option<u64>, export: &control::FlowExport) -> Handled {
        let path = self.flows_scope(caller, export.project)?;
        let (saved, note) = lookup(&self.flow_listing(path.as_deref()), &export.name)?;
        let text = share::to_share(&saved.flow)?;
        log::info!("flows", "flow exported", bytes = text.len());
        let note = [note, share::env_warning(&saved.flow)].into_iter().flatten().collect::<Vec<_>>();
        let exported = Exported {
            name: saved.flow.name.clone(),
            origin: origin_name(&saved.origin).into(),
            path: saved.origin.path().map(PathBuf::from),
            text,
            note: (!note.is_empty()).then(|| note.join("; ")),
        };
        Ok(Some(json(&exported)))
    }

    pub(super) fn flow_import_request(&mut self, caller: Option<u64>, import: &control::FlowImport) -> Handled {
        self.import_flow(caller, import).map_err(|e| share::visible(&e.message).into())
    }

    fn import_flow(&mut self, caller: Option<u64>, import: &control::FlowImport) -> Handled {
        let mut flow = share::from_share(&import.text).map_err(|e| format!("cannot read the flow: {e}"))?;
        if let Some(name) = &import.name {
            flow.name = name.trim().to_string();
        }
        let problems = flow.problems();
        if !problems.is_empty() {
            return Err(format!("the flow cannot run: {}", problems.join("; ")).into());
        }
        let to = match import.place.as_str() {
            "" | "yours" => To::Yours,
            "project" => To::Project,
            other => return Err(format!("`{other}` is not a place for a flow: use yours or project").into()),
        };
        let path = self.flows_scope(caller, import.project)?;
        if to == To::Project && path.is_none() {
            return Err(NO_PROJECT.into());
        }
        let listing = self.flow_listing(path.as_deref());
        let replaced = share::collision(&listing, &flow.name, to, import.replace)?;
        let missing = share::missing_agents(&flow, &builtin::installed(&self.config));
        let mut imported = Imported {
            name: flow.name.clone(),
            origin: if to == To::Yours { "user" } else { "project" }.into(),
            replaced: replaced.is_some(),
            note: (!missing.is_empty()).then(|| missing.join("; ")),
            path: None,
        };
        if import.check {
            return Ok(Some(json(&imported)));
        }
        let dir = match (to, &path) {
            (To::Project, Some(path)) => store::project_dir(path),
            _ => paths::flows(self.config_file.path()),
        };
        let saved =
            store::save(&dir, &flow, replaced.as_deref()).map_err(|e| format!("could not save the flow: {e}"))?;
        log::info!("flows", "flow imported", place = imported.origin, bytes = import.text.len());
        imported.path = Some(saved);
        self.refresh_flows();
        Ok(Some(json(&imported)))
    }

    fn flows_scope(&self, caller: Option<u64>, project: Option<u64>) -> Result<Option<PathBuf>, Refusal> {
        let p = match project {
            Some(id) => Some(self.projects.project_index(id).ok_or_else(|| none("project", id))?),
            None => self.here(caller).project,
        };
        Ok(p.map(|p| self.projects[p].path.clone()))
    }

    pub(super) fn flow_run_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        run: control::FlowRun,
        area: Option<Rect>,
    ) -> Handled {
        let area = sized(area)?;
        if run.issue.is_some() {
            return self.flow_issue_request(client, caller, run);
        }
        if run.source.is_some() {
            return Err(super::malformed("--source needs --issue"));
        }
        let p = match run.project {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(caller).project.ok_or(NO_PROJECT)?,
        };
        let (flow, note) = pick(&self.flow_listing(Some(&self.projects[p].path)), &run.name)?;
        let branch = run.worktree.unwrap_or_default();
        let accounts = self.requested_accounts(run.profile.as_deref(), &run.accounts)?;
        let (task, focus, trees) = (run.prompt, run.focus, Vec::new());
        if run.workspace_name.is_some() {
            return Err(super::malformed("--workspace-name needs --issue"));
        }
        let starting = Starting { flow, task, branch, focus, note, trees, parent: caller, accounts, name: None };
        self.run_flow(Some(client), self.projects[p].id, starting, area)
    }

    pub(in crate::app) fn run_flow(
        &mut self,
        client: Option<u64>,
        project: u64,
        mut starting: Starting,
        area: Rect,
    ) -> Handled {
        let p = self.projects.project_index(project).ok_or_else(|| none("project", project))?;
        starting.branch = starting.branch.trim().to_string();
        self.run_refusal(p, &starting)?;
        starting.branch = self.flow_branch(p, project, &starting)?;
        let branch = starting.branch.clone();
        let busy = |pending: &Pending| matches!(&pending.stage, Stage::Run(other, starting) if *other == project && starting.branch == branch);
        if self.clients.requests.pending.iter().any(busy) {
            return Err(format!("a flow is already starting on branch {branch}").into());
        }
        let existing = self.workspace_on(p, &branch);
        let roles: Vec<(String, String)> = starting
            .flow
            .roles
            .iter()
            .filter(|role| role.own_worktree())
            .map(|role| (role.name.clone(), flows::role_branch(&branch, &role.name)))
            .collect();
        if let Some(id) = existing {
            let at = self.usable_workspace(id)?;
            if !self.projects.workspace(at).worktree {
                return Err(own_folder(&branch).into());
            }
            if roles.is_empty() {
                return Ok(Some(json(&self.open_run(at, starting, area, false)?)));
            }
        }
        let key = self.clients.requests.key();
        let path = self.projects[p].path.clone();
        let made = existing
            .is_none()
            .then(|| worktree::checkout_path(&self.config.worktrees_dir(self.home.as_deref()), &path, &branch));
        self.spawn_run_worktrees(p, RunTrees { run: made, branch, roles }, key);
        let stage = Stage::Run(project, Box::new(starting));
        self.clients.requests.pending.push(Pending { client, key, timeout: None, done: Done::default(), stage });
        Ok(None)
    }

    fn flow_branch(&self, p: usize, project: u64, starting: &Starting) -> Result<String, String> {
        let path = self.projects[p].path.as_path();
        let branches = Branches { app: self, p, project, starting, path, own: git::checked_out(path) };
        if starting.branch.is_empty() { branches.generated() } else { branches.explicit(&starting.branch) }
    }

    pub(in crate::app) fn run_worktrees_ready(
        &mut self,
        key: u64,
        project: u64,
        result: error::Result<Made>,
        area: Rect,
    ) {
        let found =
            self.clients.requests.pending.iter().position(|p| p.key == key && matches!(p.stage, Stage::Run(..)));
        let Some(i) = found else { return };
        let pending = self.clients.requests.pending.remove(i);
        if let Stage::Run(_, starting) = pending.stage {
            self.flow_worktree_ready(Opening { client: pending.client, project, area }, result, *starting);
        }
    }

    fn flow_worktree_ready(&mut self, at: Opening, result: error::Result<Made>, starting: Starting) {
        let opened = result.map_err(|e| e.to_string()).and_then(|made| self.open_made(at, made, starting));
        match opened {
            Ok(info) => {
                if at.client.is_none() {
                    self.start_flow_settled(None);
                }
                self.answer(at.client, Ok(json(&info)));
            }
            Err(message) if at.client.is_none() => self.run_failed(&message),
            Err(message) => self.answer(at.client, Err(message)),
        }
    }

    fn open_made(&mut self, at: Opening, made: Made, mut starting: Starting) -> Result<RunInfo, String> {
        let project = at.project;
        let p = self.projects.project_index(project).ok_or_else(|| format!("project {project} closed"))?;
        if self.projects[p].closing {
            return Err(format!("project {project} is closing"));
        }
        let fresh: Vec<PathBuf> =
            made.roles.iter().filter(|(_, made)| *made).map(|(tree, _)| tree.path.clone()).collect();
        starting.trees = made.roles;
        match made.run {
            Some(path) => self.open_in_new_worktree(Landing { p, area: at.area, fresh }, path, starting),
            None => self.open_on_branch(Landing { p, area: at.area, fresh }, starting),
        }
    }

    fn open_on_branch(&mut self, landing: Landing, starting: Starting) -> Result<RunInfo, String> {
        let Landing { p, area, fresh } = landing;
        let id = self
            .workspace_on(p, &starting.branch)
            .ok_or_else(|| format!("the workspace on branch {} is gone", starting.branch))?;
        let at = self.usable_workspace(id)?;
        let opened = self.open_run(at, starting, area, false);
        if opened.is_err() {
            self.discard_worktrees(at.project(), fresh);
        }
        opened
    }

    fn open_in_new_worktree(&mut self, landing: Landing, path: PathBuf, starting: Starting) -> Result<RunInfo, String> {
        let Landing { p, area, fresh } = landing;
        let path = path.canonicalize().unwrap_or(path);
        let created = self.projects.find_workspace(p, |w| w.path == path).is_none();
        let at = self.worktree_workspace(p, path.clone());
        let opened = self.open_run(at, starting, area, true);
        if opened.is_err() {
            if created {
                self.projects[p].remove_workspace(at.index());
            }
            let run_tree = created.then_some(path).into_iter();
            self.discard_worktrees(p, fresh.into_iter().chain(run_tree).collect());
        }
        opened
    }
}

struct Landing {
    p: usize,
    area: Rect,
    fresh: Vec<PathBuf>,
}

#[derive(Clone, Copy)]
struct Opening {
    client: Option<u64>,
    project: u64,
    area: Rect,
}

struct Branches<'a> {
    app: &'a App,
    p: usize,
    project: u64,
    starting: &'a Starting,
    path: &'a Path,
    own: Option<String>,
}

impl Branches<'_> {
    fn reserved(&self, b: &str) -> bool {
        ["main", "master"].contains(&b)
            || git::origin_head(self.path).as_deref() == Some(b)
            || self.own.as_deref() == Some(b)
    }

    fn stale(&self, b: &str) -> Option<String> {
        let roles = self.starting.flow.roles.iter().filter(|role| role.own_worktree());
        roles.map(|role| flows::role_branch(b, &role.name)).find(|role_branch| git::has_branch(self.path, role_branch))
    }

    fn starting_on(&self, b: &str) -> bool {
        let on = |pending: &Pending| matches!(&pending.stage, Stage::Run(other, s) if *other == self.project && s.branch == b);
        self.app.clients.requests.pending.iter().any(on)
    }

    fn taken(&self, b: &str) -> bool {
        self.reserved(b)
            || self.app.workspace_on(self.p, b).is_some()
            || git::has_branch(self.path, b)
            || self.starting_on(b)
            || self.stale(b).is_some()
    }

    fn explicit(&self, b: &str) -> Result<String, String> {
        if self.own.as_deref() == Some(b) {
            return Err(own_folder(b));
        }
        if self.reserved(b) {
            return Err(format!(
                "branch {b} is a default branch, and a flow works on a branch of its own: pick another branch"
            ));
        }
        let free = self.app.workspace_on(self.p, b).is_none() && !git::has_branch(self.path, b);
        match self.stale(b).filter(|_| free) {
            Some(left) => Err(format!(
                "branch {left} already exists, and a new run's roles start from {b}: delete it or pick another branch"
            )),
            None => Ok(b.to_string()),
        }
    }

    fn generated(&self) -> Result<String, String> {
        let task = &self.starting.task;
        let branch = flows::branch(task);
        if branch != flows::FALLBACK_BRANCH && !self.taken(&branch) {
            return Ok(branch);
        }
        (0..u16::MAX)
            .map(|n| format!("{branch}-{:04x}", suffix(task, n)))
            .find(|b| !self.taken(b))
            .ok_or_else(|| format!("every branch named like {branch} is taken: pick a branch for the flow"))
    }
}

fn flow_info(flow: &Flow, origin: &Origin) -> FlowInfo {
    let role = |role: &Role| RoleInfo {
        role: role.name.clone(),
        agent: role.agent.clone(),
        model: role.model.clone(),
        effort: role.effort.clone(),
        edits: role.edits,
        worktree: role.worktree,
    };
    FlowInfo {
        name: flow.name.clone(),
        origin: origin_name(origin).into(),
        path: origin.path().map(PathBuf::from),
        description: flow.description.clone(),
        roles: std::iter::once(&flow.orchestrator).chain(&flow.roles).map(role).collect(),
        problems: flow.problems(),
        check: risks(flow, origin),
        flow: FlowSpec::from(flow),
        edited: false,
    }
}

fn origin_name(origin: &Origin) -> &'static str {
    match origin {
        Origin::BuiltIn => "built-in",
        Origin::User(_) => "user",
        Origin::Project(_) => "project",
    }
}

pub(super) fn pick(listing: &Listing, name: &str) -> Result<(Flow, Option<String>), String> {
    let (saved, note) = lookup(listing, name)?;
    let note = [note, risks(&saved.flow, &saved.origin)].into_iter().flatten().collect::<Vec<_>>();
    Ok((saved.flow, (!note.is_empty()).then(|| note.join(" "))))
}

fn lookup(listing: &Listing, name: &str) -> Result<(Saved, Option<String>), String> {
    let name = name.trim();
    if let Some(flow) = listing.deleted(name) {
        return Err(format!(
            "the built-in flow `{}` was deleted: bring it back in Settings › Flows, under deleted built-ins",
            flow.name.trim()
        ));
    }
    let found: Vec<&Saved> = listing.named(name).collect();
    let Some(first) = found.first() else {
        let names: Vec<&str> = listing.flows.iter().map(|saved| saved.flow.name.as_str()).collect();
        return Err(format!(
            "there is no flow named `{name}`; the flows are {} (`cornercase flow list`)",
            names.join(", ")
        ));
    };
    let note = (found.len() > 1).then(|| {
        let places: Vec<&str> = found.iter().map(|saved| saved.origin.whose()).collect();
        format!(
            "{} flows are named `{}` ({}); {} runs. Give the others names of their own to run them.",
            found.len(),
            first.flow.name,
            places.join(", "),
            first.origin.whose()
        )
    });
    Ok(((*first).clone(), note))
}

fn risks(flow: &Flow, origin: &Origin) -> Option<String> {
    matches!(origin, Origin::Project(_)).then(|| project_risks(flow)).flatten()
}

fn own_folder(branch: &str) -> String {
    format!(
        "branch {branch} is checked out in the project's own folder, and a flow works in a worktree of its own: pick \
         another branch"
    )
}

fn suffix(task: &str, n: u16) -> u16 {
    let mut hasher = DefaultHasher::new();
    (task, n).hash(&mut hasher);
    u16::try_from(hasher.finish() & 0xffff).unwrap_or_default()
}
