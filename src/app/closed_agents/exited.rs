use std::time::Instant;

use super::super::App;
use super::super::control::conflict;
use super::super::session::RESUME_GRACE;
use super::reopen::{Place, Reopened, ReopenedPane, missing_refusal};
use crate::agents::AgentState;
use crate::agents::launch::Launch;
use crate::control::contract::Refusal;
use crate::flows::run::Run;
use crate::flows::{self, Role, brief};
use crate::log::{self};
use crate::project::Hierarchy;

impl App {
    pub(in crate::app) fn role_exited(&self, pane: u64, now: Instant) -> bool {
        let Some(term) = self.projects.pane_by(pane) else { return false };
        term.agent.status().is_none()
            && term.resuming.is_none_or(|until| now >= until)
            && self.agents.launches.of_pane(pane).next().is_none()
            && !self.agents.closed.closing(pane)
            && term.shell_in_foreground()
    }

    pub(in crate::app) fn exited_panes(&self, run: &Run, now: Instant) -> Vec<u64> {
        let roles = std::iter::once(flows::ORCHESTRATOR).chain(run.flow.roles.iter().map(|role| role.name.as_str()));
        roles.filter_map(|role| run.pane_of(role)).filter(|&pane| self.role_exited(pane, now)).collect()
    }

    pub(in crate::app) fn reopenable_count(&self, workspace: u64) -> usize {
        self.agents.closed.count(workspace) + self.run_of(workspace).map_or(0, |run| run.exited.len())
    }

    pub(in crate::app) fn reopen_exited(&mut self, workspace: u64) -> Result<Reopened, Refusal> {
        let at = self.agents_workspace(workspace)?;
        let run = self.runs.of_workspace(workspace).cloned();
        let back = run.as_ref().map(|run| self.exited_panes(run, Instant::now())).unwrap_or_default();
        let Some(run) = run.filter(|_| !back.is_empty()) else {
            return Err(conflict(format!("workspace {workspace} has no closed agents")));
        };
        let kinds = back.iter().filter_map(|&pane| run.flow.role(run.role_of(pane)?)).map(|role| role.agent.as_str());
        self.agents_missing(kinds).map_err(|missing| missing_refusal(&missing))?;
        let place = Place {
            trees: self.projects.workspace(at).role_worktrees.clone(),
            path: self.projects.workspace(at).path.clone(),
        };
        let mut info = Reopened {
            project: self.projects[at.project()].id,
            workspace,
            tabs: Vec::new(),
            panes: Vec::new(),
            run: Some(run.id),
        };
        self.start_members_again(&run, &back, &place, &mut info);
        let own = run.pane_of(flows::ORCHESTRATOR).filter(|own| back.contains(own));
        if let Some(own) = own {
            self.start_orchestrator_again(&run, own, &place, &mut info);
        }
        self.settled_again(workspace, &back, own.is_some());
        log::info!(
            "agents",
            "reopen exited roles",
            workspace = workspace,
            roles = info.panes.len(),
            resumed = info.panes.iter().filter(|pane| pane.resumes).count(),
            orchestrator = own.is_some()
        );
        Ok(info)
    }

    fn settled_again(&mut self, workspace: u64, back: &[u64], orchestrator: bool) {
        let now = Instant::now();
        let Some(run) = self.runs.of_workspace_mut(workspace) else { return };
        run.exited.retain(|pane| !back.contains(pane));
        if orchestrator {
            return;
        }
        for &pane in back {
            run.brought_back(pane, now);
        }
    }

    fn exited_agent_of(&self, pane: u64, role: &Role) -> Option<AgentState> {
        let agent = self.projects.pane_by(pane)?.exited_agent.clone();
        agent.filter(|agent| agent.kind == role.agent)
    }

    fn resumed_line(&mut self, run: &Run, pane: u64, role: &Role) -> Option<(AgentState, String)> {
        let agent = self.exited_agent_of(pane, role)?;
        let line = self.resume_line(Some(role), &agent, run.dir.as_deref())?;
        let until = Instant::now() + RESUME_GRACE;
        let term = self.projects.pane_by_mut(pane)?;
        term.resume = Some(agent.clone());
        term.resuming = Some(until);
        Some((agent, line))
    }

    fn start_members_again(&mut self, run: &Run, back: &[u64], place: &Place, info: &mut Reopened) {
        for &pane in back {
            let Some(role) = run.role_of(pane).filter(|name| *name != flows::ORCHESTRATOR) else { continue };
            let Some(role) = run.flow.role(role).cloned() else { continue };
            let resumes = self.start_role_again(run, pane, &role, place);
            info.panes.push(self.reopened_pane(pane, &role, resumes));
        }
    }

    fn start_role_again(&mut self, run: &Run, pane: u64, role: &Role, place: &Place) -> bool {
        let now = Instant::now();
        let resumed = self.resumed_line(run, pane, role);
        if let Some(term) = self.projects.pane_by_mut(pane) {
            term.exited_agent = None;
        }
        if let Some((_, line)) = &resumed {
            self.agents.launches.add(Launch::resume(pane, line.clone(), true, now, now + RESUME_GRACE));
        } else {
            let launch = self.member_launch(pane, role, (&place.trees, run.dir.as_deref()));
            self.agents.launches.add(launch);
        }
        resumed.is_some()
    }

    fn start_orchestrator_again(&mut self, run: &Run, own: u64, place: &Place, info: &mut Reopened) {
        let role = &run.flow.orchestrator;
        let back: Vec<String> = info.panes.iter().filter_map(|pane| pane.role.clone()).collect();
        let fresh: Vec<String> =
            info.panes.iter().filter(|pane| !pane.resumes).filter_map(|pane| pane.role.clone()).collect();
        let notes = brief::Exited { back: &back, fresh: &fresh };
        let found = self.resumed_line(run, own, role);
        if let Some(term) = self.projects.pane_by_mut(own) {
            term.exited_agent = None;
        }
        let dir = run.dir.as_deref();
        let resumes = if let Some((_, line)) = found {
            let grace = Instant::now() + RESUME_GRACE;
            self.launch_briefed(run, (own, Some(grace)), line, brief::exited(&run.members, dir, &notes));
            true
        } else {
            self.launch_fresh_orchestrator(run, own, place, &brief::exited_stands(&run.members, dir, &notes));
            false
        };
        info.panes.push(self.reopened_pane(own, role, resumes));
    }

    fn reopened_pane(&self, pane: u64, role: &Role, resumes: bool) -> ReopenedPane {
        let tab = self.projects.tab_of_pane(pane).map(|at| self.projects.tab(at).id).unwrap_or_default();
        ReopenedPane { pane, tab, agent: Some(role.agent.clone()), role: Some(role.name.clone()), resumes }
    }
}
