use std::path::PathBuf;

use ratatui::layout::Rect;

use super::{Handled, NO_PROJECT, Pending, Stage, json, none, sized};
use crate::app::App;
use crate::app::flow_runs::Starting;
use crate::control::{self, BrokenFlow, Done, FlowInfo, Flows, RoleInfo};
use crate::error;
use crate::flows::store::{Listing, Origin, Saved};
use crate::flows::{self, Flow, Role};
use crate::project::Hierarchy;

impl App {
    pub(super) fn flow_list_request(&self, caller: Option<u64>, list: &control::FlowList) -> Handled {
        let p = match list.project {
            Some(id) => Some(self.projects.project_index(id).ok_or_else(|| none("project", id))?),
            None => self.here(caller).project,
        };
        let listing = self.flow_listing(p.map(|p| self.projects[p].path.as_path()));
        let flows = listing
            .flows
            .iter()
            .map(|saved| {
                let mut info = flow_info(&saved.flow, &saved.origin);
                let first = named(&listing, &saved.flow.name).next();
                if let Some(first) = first.filter(|first| first.origin != saved.origin) {
                    let runs = whose(&first.origin);
                    info.problems.push(format!("{runs} is also named `{}` and runs instead", saved.flow.name));
                }
                info
            })
            .collect();
        let broken = listing.broken.into_iter().map(|(path, error)| BrokenFlow { path, error }).collect();
        Ok(Some(json(&Flows { flows, broken })))
    }

    pub(super) fn flow_run_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        run: control::FlowRun,
        area: Option<Rect>,
    ) -> Handled {
        let area = sized(area)?;
        let p = match run.project {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(caller).project.ok_or(NO_PROJECT)?,
        };
        let (flow, note) = pick(&self.flow_listing(Some(&self.projects[p].path)), &run.name)?;
        let branch = run.worktree.filter(|b| !b.trim().is_empty()).unwrap_or_else(|| flows::branch(&run.prompt));
        let starting = Starting { flow, task: run.prompt, branch, focus: run.focus, note };
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
        let branch = starting.branch.clone();
        let busy = |pending: &Pending| matches!(&pending.stage, Stage::Run(other, starting) if *other == project && starting.branch == branch);
        if self.requests.pending.iter().any(busy) {
            return Err(format!("a flow is already starting on branch {branch}"));
        }
        if let Some(id) = self.workspace_on(p, &branch) {
            let (p, w) = self.usable_workspace(id)?;
            return self.open_run(p, w, starting, area).map(|info| Some(json(&info)));
        }
        let key = self.requests.key();
        self.spawn_worktree(p, branch, None, Some(key));
        let stage = Stage::Run(project, Box::new(starting));
        self.requests.pending.push(Pending { client, key, timeout: None, done: Done::default(), stage });
        Ok(None)
    }

    pub(super) fn flow_worktree_ready(
        &mut self,
        client: Option<u64>,
        project: u64,
        result: error::Result<PathBuf>,
        starting: Starting,
        area: Rect,
    ) {
        let opened = result.map_err(|e| e.to_string()).and_then(|path| {
            let path = path.canonicalize().unwrap_or(path);
            let p = self.projects.project_index(project).ok_or_else(|| format!("project {project} closed"))?;
            if self.projects[p].closing {
                return Err(format!("project {project} is closing"));
            }
            let w = self.worktree_workspace(p, path);
            self.open_run(p, w, starting, area)
        });
        match opened {
            Ok(info) => self.answer(client, Ok(json(&info))),
            Err(message) if client.is_none() => self.run_failed(&message),
            Err(message) => self.answer(client, Err(message)),
        }
    }
}

fn flow_info(flow: &Flow, origin: &Origin) -> FlowInfo {
    let role = |role: &Role| RoleInfo {
        role: role.name.clone(),
        agent: role.agent.clone(),
        model: role.model.clone(),
        edits: role.edits,
    };
    FlowInfo {
        name: flow.name.clone(),
        origin: match origin {
            Origin::BuiltIn => "built-in",
            Origin::User(_) => "user",
            Origin::Project(_) => "project",
        }
        .into(),
        path: origin.path().map(PathBuf::from),
        description: flow.description.clone(),
        roles: std::iter::once(&flow.orchestrator).chain(&flow.roles).map(role).collect(),
        problems: flow.problems(),
    }
}

fn named<'a>(listing: &'a Listing, name: &str) -> impl Iterator<Item = &'a Saved> {
    let name = name.trim().to_lowercase();
    listing.flows.iter().filter(move |saved| saved.flow.name.trim().to_lowercase() == name)
}

fn whose(origin: &Origin) -> &'static str {
    match origin {
        Origin::BuiltIn => "the built-in one",
        Origin::User(_) => "yours",
        Origin::Project(_) => "this project's",
    }
}

fn pick(listing: &Listing, name: &str) -> Result<(Flow, Option<String>), String> {
    let name = name.trim();
    let found: Vec<&Saved> = named(listing, name).collect();
    let Some(first) = found.first() else {
        let names: Vec<&str> = listing.flows.iter().map(|saved| saved.flow.name.as_str()).collect();
        return Err(format!(
            "there is no flow named `{name}`; the flows are {} (`cornercase flow list`)",
            names.join(", ")
        ));
    };
    let note = (found.len() > 1).then(|| {
        let places: Vec<&str> = found.iter().map(|saved| whose(&saved.origin)).collect();
        format!(
            "{} flows are named `{}` ({}); {} runs. Give the others names of their own to run them.",
            found.len(),
            first.flow.name,
            places.join(", "),
            whose(&first.origin)
        )
    });
    Ok((first.flow.clone(), note))
}
