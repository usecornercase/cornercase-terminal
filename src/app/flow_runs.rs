use std::path::Path;
use std::time::Instant;

use ratatui::layout::Rect;

use super::App;
use super::toast::Toast;
use crate::agents;
use crate::agents::launch::{self, Launch};
use crate::control::{MemberInfo, RunInfo};
use crate::flows::run::{Member, Run};
use crate::flows::{self, Flow, Role, brief, builtin, store};
use crate::git;
use crate::log;
use crate::paths;
use crate::project::search::Goto;
use crate::project::{Hierarchy, Phase, Tab};
use crate::state::RunState;
use crate::term::Term;
use crate::ui;

pub(super) struct Starting {
    pub(super) flow: Flow,
    pub(super) task: String,
    pub(super) branch: String,
    pub(super) focus: bool,
}

impl App {
    pub(super) fn flow_listing(&self, project: Option<&Path>) -> store::Listing {
        let project = project.map(store::project_dir);
        store::list(builtin::for_config(&self.config), &paths::flows(&self.config_path), project.as_deref())
    }

    pub(crate) fn start_run(
        &mut self,
        project: u64,
        flow: Flow,
        task: String,
        branch: String,
        focus: bool,
        area: Rect,
    ) -> Result<(), String> {
        self.run_flow(None, project, Starting { flow, task, branch, focus }, area).map(drop)
    }

    pub(super) fn run_refusal(&self, p: usize, starting: &Starting) -> Result<(), String> {
        let problems = starting.flow.problems();
        if !problems.is_empty() {
            return Err(format!("flow {} cannot run: {}", starting.flow.name, problems.join("; ")));
        }
        let flow = &starting.flow;
        let missing: Vec<String> = std::iter::once(&flow.orchestrator)
            .chain(&flow.roles)
            .map(|role| agents::command(&self.config, &role.agent))
            .filter(|program| !agents::installed(program))
            .fold(Vec::new(), |mut seen, program| {
                if !seen.contains(&program) {
                    seen.push(program);
                }
                seen
            });
        if !missing.is_empty() {
            return Err(format!(
                "flow {} needs {}, which this server cannot find on its PATH; install it, or set its command in \
                 settings → agents",
                flow.name,
                missing.join(", ")
            ));
        }
        if starting.task.trim().is_empty() {
            return Err("the flow needs a task: say what the team should do".into());
        }
        if starting.branch.trim().is_empty() {
            return Err("the flow needs a branch for its worktree".into());
        }
        let project = &self.projects[p];
        if project.closing {
            return Err(format!("project {} is closing", project.id));
        }
        if !git::is_repo_root(&project.path) {
            return Err(format!(
                "project {} is not the root of a git repository, so a flow cannot make its worktree",
                project.id
            ));
        }
        Ok(())
    }

    pub(super) fn open_run(&mut self, p: usize, w: usize, starting: Starting, area: Rect) -> Result<RunInfo, String> {
        let (project, workspace) = (&self.projects[p], &self.projects[p].workspaces[w]);
        let id = workspace.id;
        if project.closing {
            return Err(format!("project {} is closing", project.id));
        }
        match workspace.phase {
            Phase::Open => {}
            Phase::Removing(_) => return Err(format!("workspace {id} is being removed")),
            Phase::Closing => return Err(format!("workspace {id} is closing")),
        }
        if self.runs.iter().any(|run| run.workspace == id) {
            return Err(format!("workspace {id} already runs a flow, and a workspace runs one at a time"));
        }
        let Starting { flow, task, branch, focus } = starting;
        let roles: Vec<Role> = std::iter::once(flow.orchestrator.clone()).chain(flow.roles.iter().cloned()).collect();
        let mut run = Run::new(id, flow, task);
        run.id = self.take_id();
        let mut info = RunInfo { project: self.projects[p].id, workspace: id, branch, members: Vec::new() };
        for role in &roles {
            let t = self.push_tab(p, w, area, Some(role.name.clone())).map_err(|e| e.to_string())?;
            let tab = &self.projects[p].workspaces[w].tabs[t];
            let pane = tab.pane().map_or(0, |term| term.id);
            run.members.push(Member { role: role.name.clone(), pane });
            info.members.push(MemberInfo { role: role.name.clone(), agent: role.agent.clone(), tab: tab.id, pane });
        }
        let now = Instant::now();
        let mut team = Vec::new();
        for (role, member) in roles.iter().zip(&run.members).skip(1) {
            let spec = self.flow_spec(role, brief::member(role));
            self.agents.launches.push(Launch::new(member.pane, spec, now));
            team.push(member.pane);
        }
        let spec = self.flow_spec(&roles[0], brief::brief(&run.flow, &run.task, &info.branch, &run.members));
        let mut launch = Launch::new(run.members[0].pane, spec, now);
        launch.after = team;
        self.agents.launches.push(launch);
        log::info!("flow", "run started", workspace = id, flow = run.flow.name, agents = roles.len());
        self.runs.push(run);
        if focus && let Some(first) = info.members.first() {
            self.goto(Goto::Place { project: info.project, workspace: Some(id), tab: Some(first.tab) });
            self.show_flow();
        }
        Ok(info)
    }

    pub(super) fn run_failed(&mut self, message: &str) {
        log::warning!("flow", "the run did not start", error = message);
        self.toast = Some(Toast::new(format!("the flow did not start: {message}"), ui::ToastIcon::Bug));
    }

    fn flow_spec(&self, role: &Role, prompt: String) -> launch::Spec {
        let command = flows::command(&self.config, role).unwrap_or_default();
        launch::Spec { command, prompt: Some(prompt), submit: true, trusted: true }
    }

    pub(super) fn run_panes(&self, orchestrators: bool) -> Vec<u64> {
        let members = self.runs.iter().flat_map(|run| &run.members);
        members.filter(|m| (m.role == flows::ORCHESTRATOR) == orchestrators).map(|m| m.pane).collect()
    }

    pub(super) fn forget_runs(&mut self) {
        let projects = &self.projects;
        let open = |id: u64| projects.workspace_index(id).is_some_and(|(p, w)| projects[p].workspaces[w].open());
        self.runs.retain(|run| {
            let kept = open(run.workspace) && run.members.iter().any(|m| projects.pane_by(m.pane).is_some());
            if !kept {
                log::info!("flow", "run ended", workspace = run.workspace, flow = run.flow.name);
            }
            kept
        });
        self.forget_flow_reads(None);
    }

    pub(super) fn resume_line(&self, role: Option<&Role>, agent: &agents::AgentState) -> Option<String> {
        match role {
            Some(role) if role.agent == agent.kind => flows::resume(&self.config, role, agent.conversation.clone()),
            _ => agents::resume_line(&self.config, agent),
        }
    }

    pub(super) fn restore_run(&mut self, workspace: u64, saved: &RunState, tabs: &[(usize, Tab)]) {
        let mut run = Run::new(workspace, saved.flow.clone(), saved.task.clone());
        run.id = self.take_id();
        for seat in &saved.members {
            let found = tabs.iter().find(|(t, _)| *t == seat.tab).and_then(|(_, tab)| tab.panes.get(seat.pane));
            if let Some(term) = found {
                run.members.push(Member { role: seat.role.clone(), pane: term.id });
            }
        }
        log::info!("flow", "run restored", workspace = workspace, flow = run.flow.name, agents = run.members.len());
        let own = run.pane_of(flows::ORCHESTRATOR);
        if let Some(term) = tabs.iter().flat_map(|(_, tab)| &tab.panes).find(|term| Some(term.id) == own) {
            self.brief_again(&run, term);
        }
        self.runs.push(run);
    }

    fn brief_again(&mut self, run: &Run, term: &Term) {
        let briefed = |agent: &&agents::AgentState| agent.kind == run.flow.orchestrator.agent;
        let Some(agent) =
            term.resume.as_ref().filter(|_| term.resuming.is_some() && !term.agent.pending()).filter(briefed)
        else {
            return;
        };
        let Some(line) = self.resume_line(Some(&run.flow.orchestrator), agent) else { return };
        self.agents.launches.retain(|launch| launch.term != term.id);
        let spec =
            launch::Spec { command: line, prompt: Some(brief::restored(&run.members)), submit: true, trusted: true };
        let mut launch = Launch::new(term.id, spec, Instant::now());
        launch.after = run.members.iter().map(|m| m.pane).filter(|pane| *pane != term.id).collect();
        self.agents.launches.push(launch);
    }
}
