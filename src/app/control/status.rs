use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::{Handled, json, none};
use crate::agents::activity::Status;
use crate::agents::awake;
use crate::app::App;
use crate::control::{
    self, ConfigInfo, Done, GroupInfo, KeepAwake, PaneInfo, ProjectInfo, PullRequestInfo, Report, TabInfo,
    WorkspaceInfo,
};
use crate::git;
use crate::log::{self, Level};
use crate::process;
use crate::project::{Hierarchy, Tab, Workspace};
use crate::pull_request::Ci;
use crate::term::Term;
use crate::version;

impl App {
    pub(in crate::app) fn report(&self, caller: Option<u64>) -> Report {
        let caller = caller.filter(|id| self.projects.tab_of_pane(*id).is_some());
        let live = self.pane_ids();
        let tab_info = |t: usize, tab: &Tab, active: usize| TabInfo {
            id: tab.id,
            name: tab.label(&self.config),
            status: tab.status().map(|s| s.name().to_string()),
            active: t == active,
            panes: tab
                .panes
                .iter()
                .enumerate()
                .map(|(i, term)| self.pane_info(term, i == tab.active, caller, &live))
                .collect(),
            custom_name: tab.name.clone(),
            layout: Some(control::Layout::from(&tab.layout)),
            controller: self.tab_controller(tab.id),
        };
        let workspace_info = |w: usize, ws: &Workspace, active: usize| WorkspaceInfo {
            id: ws.id,
            name: ws.label(),
            path: ws.path.clone(),
            branch: git::branch(&ws.path),
            worktree: ws.worktree,
            behind: ws.behind,
            pull_request: self.pull_request(ws.id).map(|pull| PullRequestInfo {
                number: pull.number,
                title: pull.title.clone(),
                url: pull.url.clone(),
                state: pull.state.name().into(),
                checks: pull.ci().map(|ci| ci.name().into()),
                failed: pull.failed().map(|check| check.name.clone()).collect(),
                conflicts: pull.conflicts(),
                base: Some(pull.base.clone()).filter(|base| !base.is_empty()),
            }),
            setup: ws.setup().map(|(_, state)| if state.running() { "running" } else { "failed" }.to_string()),
            run_dir: self
                .runs
                .iter()
                .find(|run| run.workspace == ws.id)
                .and_then(|run| run.dir.clone())
                .or_else(|| ws.run_dirs.last().cloned()),
            active: w == active,
            tabs: ws.tabs.iter().enumerate().map(|(t, tab)| tab_info(t, tab, ws.active)).collect(),
            custom_name: ws.name.clone(),
            collapsed: ws.collapsed,
        };
        let projects = self.projects.iter().enumerate().filter(|(_, p)| !p.closing).map(|(p, project)| ProjectInfo {
            id: project.id,
            name: self.project_label(project),
            path: project.path.clone(),
            group: project.group,
            active: p == self.active,
            workspaces: project
                .workspaces
                .iter()
                .enumerate()
                .filter(|(_, w)| !w.closing())
                .map(|(w, ws)| workspace_info(w, ws, project.active))
                .collect(),
            custom_name: project.name.clone(),
            collapsed: project.collapsed,
        });
        let groups = self.groups.iter().map(|g| GroupInfo {
            id: g.id,
            name: g.entry.name.clone(),
            collapsed: g.entry.collapsed,
            icon: g.entry.icon.to_string(),
            colour: g.entry.colour,
        });
        Report {
            version: version::CURRENT.to_string(),
            boot: control::server_token().to_string(),
            caller,
            shown: self.project().map(|_| self.shown_ids(self.active)).unwrap_or_default(),
            groups: groups.collect(),
            projects: projects.collect(),
            keep_awake: self.keep_awake_info(),
            view: None,
            theme: Some(self.theme_info()),
            config: Some(ConfigInfo { revision: self.config_file.revision }),
        }
    }

    pub(in crate::app) fn refreshed(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        let live = self.pane_ids();
        for project in self.projects.iter().filter(|p| !p.closing) {
            project.id.hash(&mut hash);
            for workspace in project.workspaces.iter().filter(|w| !w.closing()) {
                (workspace.id, workspace.behind, git::branch(&workspace.path)).hash(&mut hash);
                workspace.setup().map(|(_, state)| state.running()).hash(&mut hash);
                if let Some(pull) = self.pull_request(workspace.id) {
                    (
                        pull.number,
                        &pull.title,
                        pull.state.name(),
                        pull.ci().map(Ci::name),
                        pull.conflicts(),
                        &pull.base,
                    )
                        .hash(&mut hash);
                    pull.failed().for_each(|check| check.name.hash(&mut hash));
                }
                for tab in &workspace.tabs {
                    (tab.id, tab.status().map(Status::name)).hash(&mut hash);
                    for term in &tab.panes {
                        let parent = Self::live_parent(term, &live);
                        Self::pane_refreshed(term, self.resume_holds(term.id), parent, &mut hash);
                    }
                }
            }
        }
        if let Some(awake) = self.keep_awake_info() {
            (awake.held, awake.by, awake.unavailable).hash(&mut hash);
        }
        hash.finish()
    }

    fn pane_refreshed(term: &Term, resuming: bool, parent: Option<u64>, hash: &mut DefaultHasher) {
        let pid = term.foreground_pid();
        (term.id, pid, pid.and_then(process::name), pid.and_then(process::cwd), term.shell_pid()).hash(hash);
        (term.agent.agent(), term.agent.status().map(Status::name), term.agent.background_shell()).hash(hash);
        term.context.context().map(|c| (&c.model, c.percent)).hash(hash);
        (term.dialog(), term.survey(), term.resume.is_some(), resuming, parent).hash(hash);
        term.emulator.title().hash(hash);
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
        self.events.subscribe(client, events.panes, events.host, events.notices);
        Ok(Some(json(&Done::default())))
    }
}
