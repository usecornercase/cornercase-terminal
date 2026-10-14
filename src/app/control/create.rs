use std::path::PathBuf;
use std::time::Instant;

use ratatui::layout::Rect;

use super::wait::Condition;
use super::{Handled, NO_PROJECT, Pending, Stage, deadline, json, name_of, none, sized};
use crate::agents;
use crate::agents::launch::{self, Launch};
use crate::app::App;
use crate::control::{self, Done, Ids, Item};
use crate::error;
use crate::git;
use crate::project::Hierarchy;
use crate::project::split::Dir;

pub(super) struct Then {
    start: Option<(launch::Spec, Option<String>)>,
    wait: Option<Condition>,
    focus: bool,
}

const NO_AGENT: &str = "say which agent to start, such as `cornercase start claude`: settings → agents is on auto \
    and no agent runs here";

impl App {
    pub(super) fn open_request(&mut self, open: &control::Open, area: Option<Rect>) -> Handled {
        let path = open.path.canonicalize().map_err(|e| format!("cannot open `{}`: {e}", open.path.display()))?;
        if !path.is_dir() {
            return Err(format!("`{}` is not a folder", path.display()));
        }
        let p = match self.projects.iter().position(|p| p.path == path && !p.closing) {
            Some(p) => p,
            None => self.add_project(path, sized(area)?).map_err(|e| e.to_string())?,
        };
        let ids = self.shown_ids(p);
        if open.focus {
            self.show(Item::Project(self.projects[p].id))?;
        }
        Ok(Some(json(&Done { ids, ..Done::default() })))
    }

    pub(super) fn new_workspace_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        new: control::NewWorkspace,
        area: Option<Rect>,
    ) -> Handled {
        let area = sized(area)?;
        let name = name_of(Some(new.name)).ok_or("the workspace needs a name")?;
        let p = match new.project {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(caller).project.ok_or(NO_PROJECT)?,
        };
        if new.worktree {
            if let Some(open) = self.workspace_on(p, &name) {
                return Err(format!("branch {name} is already open in workspace {open}"));
            }
            let then = Then { start: None, wait: None, focus: new.focus };
            return self.create_worktree(client, p, &name, then, None).map(|()| None);
        }
        let path = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, path, Some(name), false).map_err(|e| e.to_string())?;
        self.projects[p].workspaces.push(workspace);
        let w = self.projects[p].workspaces.len() - 1;
        let ids = self.ids(p, Some(w), Some(0));
        if new.focus {
            self.show(Item::Tab(ids.tab.unwrap_or_default()))?;
        }
        Ok(Some(json(&Done { ids, ..Done::default() })))
    }

    pub(super) fn new_tab_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        new: control::NewTab,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let (p, w) = match new.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here_workspace(caller)?,
        };
        let t = self.push_tab(p, w, area, name_of(new.name)).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        if new.focus {
            self.show(Item::Pane(pane))?;
        }
        let launch = new.command.map(|command| Launch::command(pane, command, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(None), None))
    }

    pub(super) fn split_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        split: control::Split,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let pane = self.target(caller, split.pane, None)?;
        let (p, w, t) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
        let (dir, way) = if split.down { (Dir::Down, "down") } else { (Dir::Right, "right") };
        if !self.projects[p].workspaces[w].tabs[t].can_split(self.layout(area).pane, pane, dir) {
            return Err(format!("pane {pane} is too small to split {way}"));
        }
        let new =
            self.split_pane(pane, dir, area, false).map_err(|e| e.to_string())?.ok_or_else(|| none("pane", pane))?;
        let ids = Ids { pane: Some(new), ..self.ids(p, Some(w), Some(t)) };
        if split.focus {
            self.show(Item::Pane(new))?;
        }
        let launch = split.command.map(|command| Launch::command(new, command, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(None), None))
    }

    pub(super) fn start_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        start: control::Start,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let timeout = deadline(start.timeout, now)?;
        let wait = start.wait.then(|| Condition::of(start.until)).transpose()?;
        let kind = self.agent_kind(caller, start.agent)?;
        let prompt = start.prompt.filter(|p| !p.trim().is_empty());
        let command = agents::start_line(&self.config, &kind, start.mode.as_deref(), start.model.as_deref())?;
        let spec = launch::Spec { command, prompt, submit: true, trusted: false };
        let name = name_of(start.name);
        let (p, w) = if let Some(id) = start.workspace {
            self.usable_workspace(id)?
        } else if let Some(branch) = start.worktree.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            let p = match start.project {
                Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
                None => self.here(caller).project.ok_or(NO_PROJECT)?,
            };
            let open = self.workspace_on(p, branch).and_then(|id| self.projects.workspace_index(id));
            let Some(found) = open else {
                let then = Then { start: Some((spec, name)), wait, focus: start.focus };
                return self.create_worktree(client, p, branch, then, timeout).map(|()| None);
            };
            found
        } else {
            self.here_workspace(caller)?
        };
        let t = self.push_tab(p, w, area, name).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        if start.focus {
            self.show(Item::Pane(pane))?;
        }
        let launch = Some(Launch::new(pane, spec, now));
        Ok(self.launch_within(client, Done { ids, ..Done::default() }, launch, Stage::launch(wait), timeout))
    }

    fn agent_kind(&self, caller: Option<u64>, agent: Option<String>) -> Result<String, String> {
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
        let running = self.here(caller).pane.and_then(|id| self.projects.pane_by(id));
        let running = running.and_then(|term| agents::detect(&self.config, &term.foreground_args()));
        agents::resolve(&self.config, None, running.as_deref()).ok_or_else(|| NO_AGENT.to_string())
    }

    fn create_worktree(
        &mut self,
        client: u64,
        p: usize,
        branch: &str,
        then: Then,
        timeout: Option<(Instant, f64)>,
    ) -> Result<(), String> {
        let project = &self.projects[p];
        if !git::is_repo_root(&project.path) {
            return Err(format!("project {} is not the root of a git repository, so it has no worktrees", project.id));
        }
        let key = self.requests.key();
        self.spawn_worktree(p, branch.to_string(), None, Some(key));
        let stage = Stage::Worktree(then);
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        Ok(())
    }

    pub(in crate::app) fn worktree_ready(
        &mut self,
        key: u64,
        project: u64,
        result: error::Result<PathBuf>,
        area: Rect,
    ) {
        let found = self
            .requests
            .pending
            .iter()
            .position(|p| p.key == key && matches!(p.stage, Stage::Worktree(_) | Stage::Run(_)));
        let Some(i) = found else { return };
        let mut pending = self.requests.pending.remove(i);
        let then = match std::mem::replace(&mut pending.stage, Stage::launch(None)) {
            Stage::Worktree(then) => then,
            Stage::Run(starting) => return self.flow_worktree_ready(pending.client, project, result, *starting, area),
            _ => return,
        };
        match self.open_worktree(project, result, then, area, &mut pending) {
            Ok(true) => self.requests.pending.push(pending),
            Ok(false) => self.answer(pending.client, Ok(json(&pending.done))),
            Err(message) => self.answer(pending.client, Err(message)),
        }
    }

    fn open_worktree(
        &mut self,
        project: u64,
        result: error::Result<PathBuf>,
        then: Then,
        area: Rect,
        pending: &mut Pending,
    ) -> Result<bool, String> {
        let path = result.map_err(|e| e.to_string())?;
        let path = path.canonicalize().unwrap_or(path);
        let p = self.projects.project_index(project).ok_or_else(|| format!("project {project} closed"))?;
        let w = self.worktree_workspace(p, path);
        let launch = match then.start {
            Some((spec, name)) => {
                let t = self.push_tab(p, w, area, name).map_err(|e| e.to_string())?;
                Some((spec, t))
            }
            None if self.projects[p].workspaces[w].tabs.is_empty() => {
                self.push_tab(p, w, area, None).map_err(|e| e.to_string())?;
                None
            }
            None => None,
        };
        let t = launch.as_ref().map_or(self.projects[p].workspaces[w].active, |(_, t)| *t);
        pending.done.ids = self.ids(p, Some(w), Some(t));
        let pane = pending.done.ids.pane.unwrap_or_default();
        if then.focus {
            self.show(Item::Pane(pane))?;
        }
        let Some((spec, _)) = launch else { return Ok(false) };
        let mut launch = Launch::new(pane, spec, Instant::now());
        launch.key = Some(pending.key);
        self.agents.launches.push(launch);
        pending.stage = Stage::launch(then.wait);
        Ok(true)
    }
}
