use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

use super::{Handled, json, none};
use crate::agents::activity::Status;
use crate::agents::awake;
use crate::app::App;
use crate::control::{
    self, AppState, ConfigInfo, Done, GroupInfo, KeepAwake, PaneInfo, ProjectInfo, PullRequestInfo, Report, RunStatus,
    TabInfo, WorkspaceInfo,
};
use crate::git;
use crate::log::{self, Level};
use crate::process;
use crate::project::{Group, Hierarchy, Project, Tab, Workspace};
use crate::pull_request::{Ci, PullRequest};
use crate::term::Term;
use crate::version;

struct Seen {
    caller: Option<u64>,
    live: HashSet<u64>,
    counted: Option<u64>,
    sweeping: bool,
}

fn pull_request_info(pull: &PullRequest) -> PullRequestInfo {
    PullRequestInfo {
        number: pull.number,
        title: pull.title.clone(),
        url: pull.url.clone(),
        state: pull.state.name().into(),
        checks: pull.ci().map(|ci| ci.name().into()),
        failed: pull.failed().map(|check| check.name.clone()).collect(),
        conflicts: pull.conflicts(),
        base: Some(pull.base.clone()).filter(|base| !base.is_empty()),
    }
}

fn pull_refreshed(pull: &PullRequest, hash: &mut DefaultHasher) {
    (pull.number, &pull.title, pull.state.name(), pull.ci().map(Ci::name), pull.conflicts(), &pull.base).hash(hash);
    pull.failed().for_each(|check| check.name.hash(hash));
}

impl App {
    pub(in crate::app) fn report(&self, caller: Option<u64>) -> Report {
        let seen = Seen {
            caller: caller.filter(|id| self.projects.tab_of_pane(*id).is_some()),
            live: self.pane_ids(),
            counted: self.changes_target().map(|t| t.workspace),
            sweeping: self.clients.change_streams.wants_every(),
        };
        Report {
            version: version::CURRENT.to_string(),
            boot: control::server_token().to_string(),
            caller: seen.caller,
            shown: self.project().map(|_| self.shown_ids(self.active)).unwrap_or_default(),
            groups: self.groups_info(),
            projects: self.projects_info(&seen),
            keep_awake: self.keep_awake_info(),
            view: None,
            theme: Some(self.theme_info()),
            config: Some(ConfigInfo { revision: self.config_file.revision() }),
        }
    }

    fn groups_info(&self) -> Vec<GroupInfo> {
        let info = |g: &Group| GroupInfo {
            id: g.id,
            name: g.entry.name.clone(),
            collapsed: g.entry.collapsed,
            icon: g.entry.icon.to_string(),
            colour: g.entry.colour,
        };
        self.groups.iter().map(info).collect()
    }

    fn projects_info(&self, seen: &Seen) -> Vec<ProjectInfo> {
        let shown = self.projects.iter().enumerate().filter(|(_, p)| !p.closing);
        shown.map(|(p, project)| self.project_info(project, p == self.active, seen)).collect()
    }

    fn project_info(&self, project: &Project, active: bool, seen: &Seen) -> ProjectInfo {
        let workspaces = project.workspaces.iter().enumerate().filter(|(_, w)| !w.closing());
        ProjectInfo {
            id: project.id,
            name: self.project_label(project),
            path: project.path.clone(),
            group: project.group,
            active,
            workspaces: workspaces.map(|(w, ws)| self.workspace_info(ws, w == project.active, seen)).collect(),
            custom_name: project.name.clone(),
            collapsed: project.collapsed,
        }
    }

    fn workspace_info(&self, ws: &Workspace, active: bool, seen: &Seen) -> WorkspaceInfo {
        WorkspaceInfo {
            id: ws.id,
            name: ws.label(),
            path: ws.path.clone(),
            branch: git::branch(&ws.path),
            worktree: ws.worktree,
            behind: ws.behind,
            pull_request: self.pull_request(ws.id).map(pull_request_info),
            setup: ws.setup().map(|(_, state)| if state.running() { "running" } else { "failed" }.to_string()),
            run_dir: self.workspace_run_dir(ws),
            run: self.run_status(ws.id),
            active,
            tabs: ws.tabs.iter().enumerate().map(|(t, tab)| self.tab_info(tab, t == ws.active, seen)).collect(),
            custom_name: ws.name.clone(),
            collapsed: ws.collapsed,
            changes: self.changes_count(ws.id, seen.counted, seen.sweeping),
        }
    }

    fn tab_info(&self, tab: &Tab, active: bool, seen: &Seen) -> TabInfo {
        TabInfo {
            id: tab.id,
            name: tab.label(&self.config),
            status: tab.status().map(|s| s.name().to_string()),
            active,
            panes: tab
                .panes
                .iter()
                .enumerate()
                .map(|(i, term)| self.pane_info(term, i == tab.active, seen.caller, &seen.live))
                .collect(),
            custom_name: tab.name.clone(),
            layout: Some(control::Layout::from(&tab.layout)),
            controller: self.tab_controller(tab.id),
        }
    }

    pub(in crate::app) fn app_state(&self) -> AppState {
        AppState {
            version: version::CURRENT.to_string(),
            boot: control::server_token().to_string(),
            todos: self.todo_items(),
            queue: self.panels.queued.queue.entries.clone(),
            away: self.away_info(),
            update: self.update_info(),
            keep_awake: self.keep_awake_info(),
            profile: self.config.active_profile().map(|(name, _)| name.to_string()),
            accounts: self.config.selected_accounts().clone(),
        }
    }

    fn run_status(&self, workspace: u64) -> Option<RunStatus> {
        let run = self.run_of(workspace)?;
        Some(RunStatus { end: run.flow.end.name().into(), reached: run.reached(self.pull_request(workspace)) })
    }

    pub(in crate::app) fn refreshed(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        let live = self.pane_ids();
        for project in self.projects.iter().filter(|p| !p.closing) {
            project.id.hash(&mut hash);
            for workspace in project.workspaces.iter().filter(|w| !w.closing()) {
                self.workspace_refreshed(workspace, &live, &mut hash);
            }
        }
        if let Some(awake) = self.keep_awake_info() {
            (awake.held, awake.by, awake.unavailable).hash(&mut hash);
        }
        hash.finish()
    }

    fn workspace_refreshed(&self, workspace: &Workspace, live: &HashSet<u64>, hash: &mut DefaultHasher) {
        (workspace.id, workspace.behind, git::branch(&workspace.path)).hash(hash);
        workspace.setup().map(|(_, state)| state.running()).hash(hash);
        if let Some(pull) = self.pull_request(workspace.id) {
            pull_refreshed(pull, hash);
        }
        self.run_status(workspace.id).hash(hash);
        for tab in &workspace.tabs {
            self.tab_refreshed(tab, live, hash);
        }
    }

    fn tab_refreshed(&self, tab: &Tab, live: &HashSet<u64>, hash: &mut DefaultHasher) {
        (tab.id, tab.status().map(Status::name)).hash(hash);
        for term in &tab.panes {
            let parent = Self::live_parent(term, live);
            self.seat(term.id).map(|(_, role)| role).hash(hash);
            Self::pane_refreshed(term, self.resume_holds(term.id), parent, hash);
            (self.held_sends(term.id), term.typing.drafting()).hash(hash);
        }
    }

    fn pane_refreshed(term: &Term, resuming: bool, parent: Option<u64>, hash: &mut DefaultHasher) {
        let pid = term.foreground_pid();
        (term.id, pid, pid.and_then(process::name), pid.and_then(process::cwd), term.shell_pid()).hash(hash);
        (term.agent.agent(), term.agent.status().map(Status::name), term.agent.background_shell()).hash(hash);
        term.context.context().map(|c| (&c.model, c.percent)).hash(hash);
        term.context.subagents().hash(hash);
        (term.dialog(), term.survey(), term.resume.is_some(), resuming, parent).hash(hash);
        term.emulator.title().hash(hash);
        term.account().hash(hash);
    }

    fn pane_info(&self, term: &Term, active: bool, caller: Option<u64>, live: &HashSet<u64>) -> PaneInfo {
        let context = term.context.context();
        PaneInfo {
            id: term.id,
            path: term.cwd(),
            program: term.program(&self.config),
            agent: term.agent.agent().map(str::to_string),
            status: term.agent.status().map(|s| s.name().to_string()),
            background_shell: term.agent.background_shell(),
            dialog: term.dialog(),
            survey: term.survey(),
            at_prompt: Some(term.shell_in_foreground()),
            model: context.map(|c| c.model.clone()),
            context: context.and_then(|c| c.percent),
            active,
            caller: caller == Some(term.id),
            resumes: self.config.resume_agents && term.resume.is_some(),
            resuming: self.resume_holds(term.id),
            title: Some(term.emulator.title()).filter(|title| !title.is_empty()),
            parent: Self::live_parent(term, live),
            subagents: u32::try_from(term.context.subagents()).unwrap_or(u32::MAX),
            role: self.seat(term.id).map(|(_, role)| role.to_string()),
            held_sends: u32::try_from(self.held_sends(term.id)).unwrap_or(u32::MAX),
            draft: term.draft(Instant::now()),
            collapsed: term.collapsed,
            account: term.account().map(str::to_string),
        }
    }

    fn keep_awake_info(&self) -> Option<KeepAwake> {
        match self.agents.awake.state() {
            awake::State::Off => None,
            awake::State::Released => Some(KeepAwake::default()),
            awake::State::Held(by) => Some(KeepAwake { held: true, by: Some(by), unavailable: None }),
            awake::State::Unavailable(why) => Some(KeepAwake { held: false, by: None, unavailable: Some(why) }),
        }
    }

    pub(super) fn events_request(&mut self, client: u64, events: control::Events) -> Handled {
        if let Some(pane) = events.panes.iter().find(|pane| self.projects.tab_of_pane(**pane).is_none()) {
            return Err(none("pane", *pane));
        }
        self.publish(log::enabled(Level::Info));
        self.clients.events.subscribe(client, events.panes, events.host, events.notices);
        Ok(Some(json(&Done::default())))
    }
}
