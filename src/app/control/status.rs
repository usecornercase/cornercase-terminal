use super::{Handled, json, none};
use crate::agents::awake;
use crate::app::App;
use crate::control::{
    self, Done, GroupInfo, KeepAwake, PaneInfo, ProjectInfo, PullRequestInfo, Report, TabInfo, WorkspaceInfo,
};
use crate::git;
use crate::log::{self, Level};
use crate::project::{Hierarchy, Tab, Workspace};
use crate::term::Term;
use crate::version;

impl App {
    pub(in crate::app) fn report(&self, caller: Option<u64>) -> Report {
        let caller = caller.filter(|id| self.projects.tab_of_pane(*id).is_some());
        let tab_info = |t: usize, tab: &Tab, active: usize| TabInfo {
            id: tab.id,
            name: tab.label(&self.config),
            status: tab.status().map(|s| s.name().to_string()),
            active: t == active,
            panes: tab
                .panes
                .iter()
                .enumerate()
                .map(|(i, term)| self.pane_info(term, i == tab.active, caller))
                .collect(),
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
            }),
            setup: ws.setup().map(|(_, state)| if state.running() { "running" } else { "failed" }.to_string()),
            active: w == active,
            tabs: ws.tabs.iter().enumerate().map(|(t, tab)| tab_info(t, tab, ws.active)).collect(),
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
        });
        let groups = self.groups.iter().map(|g| GroupInfo {
            id: g.id,
            name: g.entry.name.clone(),
            collapsed: g.entry.collapsed,
        });
        Report {
            version: version::CURRENT.to_string(),
            caller,
            shown: self.project().map(|_| self.shown_ids(self.active)).unwrap_or_default(),
            groups: groups.collect(),
            projects: projects.collect(),
            keep_awake: self.keep_awake_info(),
        }
    }

    fn pane_info(&self, term: &Term, active: bool, caller: Option<u64>) -> PaneInfo {
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
        self.events.subscribe(client, events.panes);
        Ok(Some(json(&Done::default())))
    }
}
