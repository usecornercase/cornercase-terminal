use std::path::PathBuf;
use std::time::Instant;

use ratatui::layout::Rect;

use super::wait::Condition;
use super::{Asked, Asker, Handled, NO_PROJECT, Pending, Stage, Waits, deadline, json, name_of, none, sized};
use crate::accounts::Accounts;
use crate::agents;
use crate::agents::launch::{self, Launch};
use crate::app::App;
use crate::config;
use crate::control::contract::Refusal;
use crate::control::{self, Done, Ids, Item};
use crate::error;
use crate::git;
use crate::project::split::Dir;
use crate::project::{Hierarchy, WorkspaceAt};

pub(super) struct Within {
    pub(super) timeout: Option<(Instant, f64)>,
    pub(super) named: Option<String>,
    pub(super) accounts: Accounts,
}

enum Place {
    Open(WorkspaceAt),
    Create(usize, String),
}

#[derive(Clone, Copy)]
struct Worktree<'a> {
    project: usize,
    branch: &'a str,
}

pub(super) struct Then {
    start: Option<(launch::Spec, Option<String>)>,
    wait: Option<Condition>,
    focus: bool,
    view: Option<u64>,
    parent: Option<u64>,
    named: Option<String>,
    accounts: Accounts,
}

const NO_AGENT: &str = "say which agent to start, such as `cornercase start claude`: settings → agents is on auto \
    and no agent runs here";

impl App {
    pub(super) fn open_request(&mut self, asker: Asker, open: &control::Open, area: Option<Rect>) -> Handled {
        let wanted = config::expand_home(&open.path.to_string_lossy(), self.home.as_deref());
        let path = wanted.canonicalize().map_err(|e| format!("cannot open `{}`: {e}", open.path.display()))?;
        if !path.is_dir() {
            return Err(format!("`{}` is not a folder", path.display()).into());
        }
        let p = match self.projects.iter().position(|p| p.path == path && !p.closing) {
            Some(p) => p,
            None => self.add_project(path, sized(area)?).map_err(|e| e.to_string())?,
        };
        let ids = self.shown_ids(p);
        let projection = self.show_if(asker.view, open.focus, Item::Project(self.projects[p].id))?;
        Ok(Some(json(&Done { ids, projection, ..Done::default() })))
    }

    pub(super) fn project_of(&self, asker: Asker, id: Option<u64>) -> Result<usize, Refusal> {
        match id {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id)),
            None => Ok(self.here(asker).project.ok_or(NO_PROJECT)?),
        }
    }

    pub(super) fn new_workspace_request(
        &mut self,
        client: u64,
        asker: Asker,
        new: control::NewWorkspace,
        area: Option<Rect>,
    ) -> Handled {
        let area = sized(area)?;
        let name = name_of(Some(new.name)).ok_or("the workspace needs a name")?;
        let p = self.project_of(asker, new.project)?;
        if new.worktree {
            if let Some(open) = self.workspace_on(p, &name) {
                return Err(format!("branch {name} is already open in workspace {open}").into());
            }
            let accounts = self.active_accounts();
            let (focus, view) = (new.focus, asker.view);
            let then = Then { start: None, wait: None, focus, view, parent: None, named: None, accounts };
            self.create_worktree(client, Worktree { project: p, branch: &name }, then, None)?;
            return Ok(None);
        }
        let path = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, path, Some(name), false).map_err(|e| e.to_string())?;
        let at = self.projects.push_workspace(p, workspace);
        let ids = self.ids(p, Some(at), self.projects.tab_at(p, at.index(), 0));
        let projection = self.show_if(asker.view, new.focus, Item::Tab(ids.tab.unwrap_or_default()))?;
        Ok(Some(json(&Done { ids, projection, ..Done::default() })))
    }

    pub(super) fn new_tab_request(&mut self, asked: Asked, new: control::NewTab) -> Handled {
        let Asked { client, asker, area, now } = asked;
        let area = sized(area)?;
        let at = match new.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here_workspace(asker)?,
        };
        let accounts = self.requested_accounts_from(asker.caller, new.profile.as_deref(), &new.accounts)?;
        let tab = self.push_tab_in(at, area, name_of(new.name), accounts).map_err(|e| e.to_string())?;
        let ids = self.ids_of_tab(tab);
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        let projection = self.show_if(asker.view, new.focus, Item::Pane(pane))?;
        let launch = new.command.map(|command| Launch::command(pane, command, now));
        let waits =
            Waits { done: Done { ids, projection, ..Done::default() }, stage: Stage::launch(None), timeout: None };
        Ok(self.launch_within(client, launch, waits))
    }

    pub(super) fn split_request(&mut self, asked: Asked, split: control::Split) -> Handled {
        let Asked { client, asker, area, now } = asked;
        let area = sized(area)?;
        let pane = self.target(asker, split.pane, None)?;
        let at = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
        let (dir, way) = if split.down { (Dir::Down, "down") } else { (Dir::Right, "right") };
        let tab = self.projects.tab(at);
        if !tab.can_split(self.tab_area(tab.id, area), pane, dir) {
            return Err(format!("pane {pane} is too small to split {way}").into());
        }
        let new =
            self.split_pane(pane, dir, area, false).map_err(|e| e.to_string())?.ok_or_else(|| none("pane", pane))?;
        let ids = Ids { pane: Some(new), ..self.ids_of_tab(at) };
        self.adopt(new, asker.caller);
        let projection = self.show_if(asker.view, split.focus, Item::Pane(new))?;
        let launch = split.command.map(|command| Launch::command(new, command, now));
        let waits =
            Waits { done: Done { ids, projection, ..Done::default() }, stage: Stage::launch(None), timeout: None };
        Ok(self.launch_within(client, launch, waits))
    }

    pub(super) fn start_request(&mut self, asked: Asked, start: control::Start) -> Handled {
        let area = sized(asked.area)?;
        let timeout = deadline(start.timeout, asked.now)?;
        let accounts = self.requested_accounts_from(asked.asker.caller, start.profile.as_deref(), &start.accounts)?;
        if start.issue.is_some() {
            if start.wait {
                Condition::of(start.until.clone())?;
            }
            return self.start_issue_request(
                asked.client,
                asked.asker,
                start,
                Within { timeout, named: None, accounts },
            );
        }
        if start.source.is_some() {
            return Err(super::malformed("--source needs --issue"));
        }
        let asked = Asked { area: Some(area), ..asked };
        self.start_within(asked, &start, Within { timeout, named: None, accounts })
    }

    pub(super) fn start_within(&mut self, asked: Asked, start: &control::Start, within: Within) -> Handled {
        let Asked { client, asker, area, now } = asked;
        let Within { timeout, named, accounts } = within;
        let area = sized(area)?;
        let wait = start.wait.then(|| Condition::of(start.until.clone())).transpose()?;
        let spec = self.start_spec(client, asker, start)?;
        let name = name_of(start.name.clone());
        let at = match self.start_place(asker, start, named.as_deref())? {
            Place::Open(at) => at,
            Place::Create(p, branch) => {
                let (focus, view, parent) = (start.focus, asker.view, asker.caller);
                let then = Then { start: Some((spec, name)), wait, focus, view, parent, named, accounts };
                self.create_worktree(client, Worktree { project: p, branch: &branch }, then, timeout)?;
                return Ok(None);
            }
        };
        let tab = self.push_tab_in(at, area, name, accounts).map_err(|e| e.to_string())?;
        let ids = self.ids_of_tab(tab);
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        let projection = self.show_if(asker.view, start.focus, Item::Pane(pane))?;
        let launch = Some(Launch::new(pane, spec, now));
        let waits = Waits { done: Done { ids, projection, ..Done::default() }, stage: Stage::launch(wait), timeout };
        Ok(self.launch_within(client, launch, waits))
    }

    fn start_spec(&self, client: u64, asker: Asker, start: &control::Start) -> Result<launch::Spec, Refusal> {
        if let Some(spec) = self.queued_spec(client) {
            return Ok(spec);
        }
        let kind = self.agent_kind(asker, start.agent.clone())?;
        let prompt = start.prompt.clone().filter(|p| !p.trim().is_empty());
        let (mode, model, effort) = (start.mode.as_deref(), start.model.as_deref(), start.effort.as_deref());
        let command = agents::start_line(&self.config, &kind, mode, model, effort)?;
        Ok(launch::Spec { command, prompt, submit: true, trusted: false })
    }

    fn start_place(&mut self, asker: Asker, start: &control::Start, named: Option<&str>) -> Result<Place, Refusal> {
        if let Some(id) = start.workspace {
            return self.usable_workspace(id).map(Place::Open);
        }
        if let Some(branch) = start.worktree.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            let p = self.project_of(asker, start.project)?;
            let open = self.workspace_on(p, branch).and_then(|id| self.projects.workspace_index(id));
            if let (Some(at), Some(name)) = (open, named) {
                self.name_workspace(at, name);
            }
            return Ok(open.map_or_else(|| Place::Create(p, branch.to_string()), Place::Open));
        }
        if let Some(project) = start.project {
            return self.root_workspace(project).map(Place::Open);
        }
        Ok(Place::Open(self.here_workspace(asker)?))
    }

    fn root_workspace(&mut self, project: u64) -> Result<WorkspaceAt, Refusal> {
        let p = self.projects.project_index(project).ok_or_else(|| none("project", project))?;
        let path = self.projects[p].path.clone();
        if let Some(at) = self.projects.find_workspace(p, |workspace| workspace.open() && workspace.path == path) {
            return Ok(at);
        }
        let id = self.take_id();
        Ok(self.projects.push_workspace(p, crate::project::Workspace::new(id, path, None, false)))
    }

    fn name_workspace(&mut self, at: WorkspaceAt, name: &str) {
        let workspace = self.projects.workspace_mut(at);
        if workspace.name.is_none() {
            workspace.name = Some(name.to_string());
        }
    }

    pub(in crate::app) fn adopt(&mut self, pane: u64, parent: Option<u64>) {
        let parent = parent.filter(|&id| id != pane && self.projects.pane_by(id).is_some());
        if let Some(term) = self.projects.pane_by_mut(pane) {
            term.parent = parent;
        }
    }

    pub(in crate::app) fn agent_kind(&self, asker: impl Into<Asker>, agent: Option<String>) -> Result<String, String> {
        let kinds = agents::kinds(&self.config);
        if let Some(agent) = agent {
            return if kinds.contains(&agent) {
                Ok(agent)
            } else {
                Err(format!(
                    "unknown agent `{agent}`; cornercase knows {}, and more can be added with agent_commands in \
                     config.json",
                    kinds.join(", ")
                ))
            };
        }
        let running = self.here(asker).pane.and_then(|id| self.projects.pane_by(id));
        let running = running.and_then(|term| agents::detect(&self.config, &term.foreground_args()));
        agents::resolve(&self.config, None, running.as_deref()).ok_or_else(|| NO_AGENT.to_string())
    }

    fn create_worktree(
        &mut self,
        client: u64,
        at: Worktree,
        then: Then,
        timeout: Option<(Instant, f64)>,
    ) -> Result<(), String> {
        let Worktree { project: p, branch } = at;
        let project = &self.projects[p];
        if !git::is_repo_root(&project.path) {
            return Err(format!("project {} is not the root of a git repository, so it has no worktrees", project.id));
        }
        let key = self.clients.requests.key();
        self.spawn_worktree(p, branch.to_string(), None, Some(key));
        let waits = Waits { done: Done::default(), stage: Stage::Worktree(then), timeout };
        self.clients.requests.push(client, key, waits);
        Ok(())
    }

    pub(in crate::app) fn worktree_ready(
        &mut self,
        key: u64,
        project: u64,
        result: error::Result<PathBuf>,
        area: Rect,
    ) {
        let found =
            self.clients.requests.pending.iter().position(|p| p.key == key && matches!(p.stage, Stage::Worktree(_)));
        let Some(i) = found else { return };
        let mut pending = self.clients.requests.pending.remove(i);
        let Stage::Worktree(then) = std::mem::replace(&mut pending.stage, Stage::launch(None)) else { return };
        let opened =
            self.worktree_place(project, result).and_then(|place| self.open_worktree(place, then, area, &mut pending));
        match opened {
            Ok(true) => self.clients.requests.pending.push(pending),
            Ok(false) => self.answer(pending.client, Ok(json(&pending.done))),
            Err(message) => self.answer(pending.client, Err(message)),
        }
    }

    fn worktree_place(&mut self, project: u64, result: error::Result<PathBuf>) -> Result<WorkspaceAt, String> {
        let path = result.map_err(|e| e.to_string())?;
        let path = path.canonicalize().unwrap_or(path);
        let p = self.projects.project_index(project).ok_or_else(|| format!("project {project} closed"))?;
        Ok(self.worktree_workspace(p, path))
    }

    fn open_worktree(
        &mut self,
        at: WorkspaceAt,
        then: Then,
        area: Rect,
        pending: &mut Pending,
    ) -> Result<bool, String> {
        if let Some(name) = &then.named {
            self.name_workspace(at, name);
        }
        let launch = match then.start {
            Some((spec, name)) => {
                let tab = self.push_setup_tab_in(at, area, name, then.accounts.clone()).map_err(|e| e.to_string())?;
                Some((spec, tab))
            }
            None if self.projects.workspace(at).tabs.is_empty() => {
                self.push_setup_tab_in(at, area, None, then.accounts.clone()).map_err(|e| e.to_string())?;
                None
            }
            None => None,
        };
        let tab = launch.as_ref().map_or_else(|| self.projects.active_tab(at), |(_, tab)| Some(*tab));
        pending.done.ids = self.ids(at.project(), Some(at), tab);
        let pane = pending.done.ids.pane.unwrap_or_default();
        if launch.is_some() {
            self.adopt(pane, then.parent);
        }
        if then.focus {
            pending.done.projection = self.focus_in_view(then.view, pane)?;
        }
        let Some((spec, _)) = launch else {
            return Ok(self.settle_worktree(at, pending));
        };
        self.launch_in_worktree(spec, pane, then.wait, pending);
        Ok(true)
    }

    fn settle_worktree(&self, at: WorkspaceAt, pending: &mut Pending) -> bool {
        let workspace = self.projects.workspace(at);
        if workspace.setting_up() {
            pending.stage = Stage::Setup(workspace.id);
        }
        workspace.setting_up()
    }

    fn launch_in_worktree(&mut self, spec: launch::Spec, pane: u64, wait: Option<Condition>, pending: &mut Pending) {
        let mut launch = Launch::new(pane, spec, Instant::now());
        launch.key = Some(pending.key);
        launch.trust =
            pending.client.is_some_and(|client| self.panels.queued.trust(client)).then_some(launch::Trust::Accepted);
        self.agents.launches.add(launch);
        pending.stage = Stage::launch(wait);
    }
}
