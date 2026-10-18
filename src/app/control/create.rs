use std::path::PathBuf;
use std::time::Instant;

use ratatui::layout::Rect;

use super::wait::Condition;
use super::{Asker, Handled, NO_PROJECT, Pending, Stage, deadline, json, name_of, none, sized};
use crate::accounts::Accounts;
use crate::agents;
use crate::agents::launch::{self, Launch};
use crate::app::App;
use crate::control::{self, Done, Ids, Item};
use crate::error;
use crate::git;
use crate::project::Hierarchy;
use crate::project::split::Dir;

pub(super) struct Within {
    pub(super) timeout: Option<(Instant, f64)>,
    pub(super) named: Option<String>,
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
        let path = open.path.canonicalize().map_err(|e| format!("cannot open `{}`: {e}", open.path.display()))?;
        if !path.is_dir() {
            return Err(format!("`{}` is not a folder", path.display()).into());
        }
        let p = match self.projects.iter().position(|p| p.path == path && !p.closing) {
            Some(p) => p,
            None => self.add_project(path, sized(area)?).map_err(|e| e.to_string())?,
        };
        let ids = self.shown_ids(p);
        let mut projection = None;
        if open.focus {
            projection = self.show_to(asker.view, Item::Project(self.projects[p].id))?;
        }
        Ok(Some(json(&Done { ids, projection, ..Done::default() })))
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
        let p = match new.project {
            Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
            None => self.here(asker).project.ok_or(NO_PROJECT)?,
        };
        if new.worktree {
            if let Some(open) = self.workspace_on(p, &name) {
                return Err(format!("branch {name} is already open in workspace {open}").into());
            }
            let accounts = self.active_accounts();
            let (focus, view) = (new.focus, asker.view);
            let then = Then { start: None, wait: None, focus, view, parent: None, named: None, accounts };
            self.create_worktree(client, p, &name, then, None)?;
            return Ok(None);
        }
        let path = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, path, Some(name), false).map_err(|e| e.to_string())?;
        self.projects[p].workspaces.push(workspace);
        let w = self.projects[p].workspaces.len() - 1;
        let ids = self.ids(p, Some(w), Some(0));
        let mut projection = None;
        if new.focus {
            projection = self.show_to(asker.view, Item::Tab(ids.tab.unwrap_or_default()))?;
        }
        Ok(Some(json(&Done { ids, projection, ..Done::default() })))
    }

    pub(super) fn new_tab_request(
        &mut self,
        client: u64,
        asker: Asker,
        new: control::NewTab,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let (p, w) = match new.workspace {
            Some(id) => self.usable_workspace(id)?,
            None => self.here_workspace(asker)?,
        };
        let accounts = self.profile_accounts(new.profile.as_deref())?;
        let t = self.push_tab_in(p, w, area, name_of(new.name), accounts).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        let mut projection = None;
        if new.focus {
            projection = self.show_to(asker.view, Item::Pane(pane))?;
        }
        let launch = new.command.map(|command| Launch::command(pane, command, now));
        Ok(self.launch_within(client, Done { ids, projection, ..Done::default() }, launch, Stage::launch(None), None))
    }

    pub(super) fn split_request(
        &mut self,
        client: u64,
        asker: Asker,
        split: control::Split,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let pane = self.target(asker, split.pane, None)?;
        let (p, w, t) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
        let (dir, way) = if split.down { (Dir::Down, "down") } else { (Dir::Right, "right") };
        if !self.projects[p].workspaces[w].tabs[t].can_split(
            self.tab_area(self.projects[p].workspaces[w].tabs[t].id, area),
            pane,
            dir,
        ) {
            return Err(format!("pane {pane} is too small to split {way}").into());
        }
        let new =
            self.split_pane(pane, dir, area, false).map_err(|e| e.to_string())?.ok_or_else(|| none("pane", pane))?;
        let ids = Ids { pane: Some(new), ..self.ids(p, Some(w), Some(t)) };
        self.adopt(new, asker.caller);
        let mut projection = None;
        if split.focus {
            projection = self.show_to(asker.view, Item::Pane(new))?;
        }
        let launch = split.command.map(|command| Launch::command(new, command, now));
        Ok(self.launch_within(client, Done { ids, projection, ..Done::default() }, launch, Stage::launch(None), None))
    }

    pub(super) fn start_request(
        &mut self,
        client: u64,
        asker: Asker,
        start: control::Start,
        area: Option<Rect>,
        now: Instant,
    ) -> Handled {
        let area = sized(area)?;
        let timeout = deadline(start.timeout, now)?;
        self.profile_accounts(start.profile.as_deref())?;
        if start.issue.is_some() {
            if start.wait {
                Condition::of(start.until.clone())?;
            }
            return self.start_issue_request(client, asker, start, timeout);
        }
        if start.source.is_some() {
            return Err(super::malformed("--source needs --issue"));
        }
        self.start_within(client, asker, start, Some(area), now, Within { timeout, named: None })
    }

    pub(super) fn start_within(
        &mut self,
        client: u64,
        asker: Asker,
        start: control::Start,
        area: Option<Rect>,
        now: Instant,
        within: Within,
    ) -> Handled {
        let Within { timeout, named } = within;
        let area = sized(area)?;
        let wait = start.wait.then(|| Condition::of(start.until)).transpose()?;
        let accounts = self.profile_accounts(start.profile.as_deref())?;
        let queued = self.queued_spec(client);
        let kind =
            if queued.is_some() { start.agent.unwrap_or_default() } else { self.agent_kind(asker, start.agent)? };
        let prompt = start.prompt.filter(|p| !p.trim().is_empty());
        let spec = if let Some(spec) = queued {
            spec
        } else {
            let (mode, model, effort) = (start.mode.as_deref(), start.model.as_deref(), start.effort.as_deref());
            let command = agents::start_line(&self.config, &kind, mode, model, effort)?;
            launch::Spec { command, prompt, submit: true, trusted: false }
        };
        let name = name_of(start.name);
        let (p, w) = if let Some(id) = start.workspace {
            self.usable_workspace(id)?
        } else if let Some(branch) = start.worktree.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
            let p = match start.project {
                Some(id) => self.projects.project_index(id).ok_or_else(|| none("project", id))?,
                None => self.here(asker).project.ok_or(NO_PROJECT)?,
            };
            let open = self.workspace_on(p, branch).and_then(|id| self.projects.workspace_index(id));
            if let (Some((p, w)), Some(name)) = (open, &named) {
                self.name_workspace(p, w, name);
            }
            let Some(found) = open else {
                let then = Then {
                    start: Some((spec, name)),
                    wait,
                    focus: start.focus,
                    view: asker.view,
                    parent: asker.caller,
                    named,
                    accounts,
                };
                self.create_worktree(client, p, branch, then, timeout)?;
                return Ok(None);
            };
            found
        } else if let Some(project) = start.project {
            let p = self.projects.project_index(project).ok_or_else(|| none("project", project))?;
            let w = self.projects[p]
                .workspaces
                .iter()
                .position(|workspace| workspace.open() && workspace.path == self.projects[p].path)
                .unwrap_or_else(|| {
                    let id = self.take_id();
                    let path = self.projects[p].path.clone();
                    self.projects[p].workspaces.push(crate::project::Workspace::new(id, path, None, false));
                    self.projects[p].workspaces.len() - 1
                });
            (p, w)
        } else {
            self.here_workspace(asker)?
        };
        let t = self.push_tab_in(p, w, area, name, accounts).map_err(|e| e.to_string())?;
        let ids = self.ids(p, Some(w), Some(t));
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        let mut projection = None;
        if start.focus {
            projection = self.show_to(asker.view, Item::Pane(pane))?;
        }
        let launch = Some(Launch::new(pane, spec, now));
        Ok(self.launch_within(
            client,
            Done { ids, projection, ..Done::default() },
            launch,
            Stage::launch(wait),
            timeout,
        ))
    }

    fn name_workspace(&mut self, p: usize, w: usize, name: &str) {
        let workspace = &mut self.projects[p].workspaces[w];
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
        let found = self.requests.pending.iter().position(|p| p.key == key && matches!(p.stage, Stage::Worktree(_)));
        let Some(i) = found else { return };
        let mut pending = self.requests.pending.remove(i);
        let Stage::Worktree(then) = std::mem::replace(&mut pending.stage, Stage::launch(None)) else { return };
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
        if let Some(name) = &then.named {
            self.name_workspace(p, w, name);
        }
        let launch = match then.start {
            Some((spec, name)) => {
                let t = self.push_setup_tab_in(p, w, area, name, then.accounts.clone()).map_err(|e| e.to_string())?;
                Some((spec, t))
            }
            None if self.projects[p].workspaces[w].tabs.is_empty() => {
                self.push_setup_tab_in(p, w, area, None, then.accounts.clone()).map_err(|e| e.to_string())?;
                None
            }
            None => None,
        };
        let t = launch.as_ref().map_or(self.projects[p].workspaces[w].active, |(_, t)| *t);
        pending.done.ids = self.ids(p, Some(w), Some(t));
        let pane = pending.done.ids.pane.unwrap_or_default();
        if launch.is_some() {
            self.adopt(pane, then.parent);
        }
        if then.focus {
            pending.done.projection = self.focus_in_view(then.view, pane)?;
        }
        let Some((spec, _)) = launch else {
            let workspace = self.projects[p].workspaces[w].id;
            if self.projects[p].workspaces[w].setting_up() {
                pending.stage = Stage::Setup(workspace);
                return Ok(true);
            }
            return Ok(false);
        };
        let mut launch = Launch::new(pane, spec, Instant::now());
        launch.key = Some(pending.key);
        launch.trust = pending
            .client
            .is_some_and(|client| self.queued.running.contains_key(&client))
            .then_some(launch::Trust::Accepted);
        self.agents.launches.push(launch);
        pending.stage = Stage::launch(then.wait);
        Ok(true)
    }
}
