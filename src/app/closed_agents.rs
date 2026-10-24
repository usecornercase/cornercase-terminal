use ratatui::layout::Rect;

use super::App;
use super::control::conflict;
use super::menus::MenuAction;
use super::toast::{Toast, Undo};
use crate::agents::activity::Status;
use crate::agents::{self};
use crate::control::contract::Refusal;
use crate::flows::run::Run;
use crate::log::{self};
use crate::project::{Hierarchy, Project, Tab, WorkspaceAt};
use crate::state::{ClosedAgent, SeatState, TabState};
use crate::term::Term;
use crate::ui;

pub(super) mod dialog;
mod kept;
pub(super) mod reopen;
mod session;

pub(super) use kept::ClosedAgents;
pub(super) use kept::{Kept, KeptRun};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::app) struct Plan {
    pub(in crate::app) place: (u64, u64),
    pub(in crate::app) tabs: Vec<u64>,
    pub(in crate::app) agents: Vec<u64>,
    pub(in crate::app) cut: Vec<u64>,
    pub(in crate::app) fresh: Vec<u64>,
    pub(in crate::app) others: Vec<u64>,
    pub(in crate::app) run: Option<u64>,
}

impl Plan {
    pub(in crate::app) fn asks(&self) -> bool {
        !(self.cut.is_empty() && self.fresh.is_empty() && self.others.is_empty())
    }

    pub(in crate::app) fn grew(&self, shown: &Self) -> bool {
        let new = |now: &[u64], then: &[u64]| now.iter().any(|id| !then.contains(id));
        self.run != shown.run
            || new(&self.tabs, &shown.tabs)
            || new(&self.cut, &shown.cut)
            || new(&self.fresh, &shown.fresh)
            || new(&self.others, &shown.others)
    }
}

pub(in crate::app) fn agents_label(count: usize) -> String {
    if count == 1 { "1 agent".to_string() } else { format!("{count} agents") }
}

const NO_AGENT_TO_CLOSE: &str = "no agent to close in this workspace";

const NO_CLOSED_AGENTS: &str = "this workspace has no closed agents";

pub(in crate::app) fn forgotten(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => " Its closed agent is forgotten.".to_string(),
        n => format!(" Its {n} closed agents are forgotten."),
    }
}

pub(in crate::app) fn closed_message(count: usize) -> String {
    let (what, them) = if count == 1 { ("agent", "it") } else { ("agents", "them") };
    format!("{count} {what} closed · {} on the workspace reopens {them}", ui::REOPEN_ICON)
}

impl App {
    pub(in crate::app) fn agents_workspace(&self, workspace: u64) -> Result<WorkspaceAt, Refusal> {
        let at = self.usable_workspace(workspace)?;
        let project = &self.projects[at.project()];
        if project.closing {
            return Err(conflict(format!("project {} is closing", project.id)));
        }
        Ok(at)
    }

    pub(in crate::app) fn agents_close_plan(&self, workspace: u64) -> Result<Plan, Refusal> {
        let at = self.agents_workspace(workspace)?;
        let project = &self.projects[at.project()];
        let run = self.run_of(workspace);
        let mut plan = Plan { place: (project.id, workspace), run: run.map(|run| run.id), ..Plan::default() };
        for tab in &self.projects.workspace(at).tabs {
            self.plan_tab(&mut plan, tab, run);
        }
        if plan.tabs.is_empty() {
            return Err(conflict(format!("workspace {workspace} runs no agent cornercase reads")));
        }
        Ok(plan)
    }

    fn plan_tab(&self, plan: &mut Plan, tab: &Tab, run: Option<&Run>) {
        let dying = |term: &&Term| !self.agents.closed.closing(term.id);
        let seated = |term: &Term| run.is_some_and(|run| run.role_of(term.id).is_some());
        let (agents, others): (Vec<&Term>, Vec<&Term>) =
            tab.panes.iter().filter(dying).partition(|term| term.agent.status().is_some() || seated(term));
        if agents.is_empty() {
            return;
        }
        plan.tabs.push(tab.id);
        plan.others.extend(others.iter().map(|term| term.id));
        for term in agents {
            plan.agents.push(term.id);
            if matches!(term.agent.status(), Some(Status::Working | Status::Waiting)) {
                plan.cut.push(term.id);
            }
            if self.starts_over(term, run) {
                plan.fresh.push(term.id);
            }
        }
    }

    fn starts_over(&self, term: &Term, run: Option<&Run>) -> bool {
        let role = run.and_then(|run| run.flow.role(run.role_of(term.id)?));
        let dir = run.and_then(|run| run.dir.as_deref());
        term.resume.as_ref().and_then(|agent| self.resume_line(role, agent, dir)).is_none()
    }

    pub(in crate::app) fn close_agents(&mut self, plan: &Plan) {
        let live = self.pane_ids();
        let run = plan.run.and_then(|_| self.runs.take_of(plan.place.1));
        let (mut tabs, mut seats, mut panes) = (Vec::new(), Vec::new(), Vec::new());
        for &id in &plan.tabs {
            let Some(at) = self.projects.tab_index(id) else { continue };
            let tab = self.projects.tab(at);
            if let Some(run) = &run {
                seats.extend(seats_in(run, tab, tabs.len()));
            }
            tabs.push(self.closed_tab(tab, plan, run.as_ref(), &live));
            panes.extend(tab.panes.iter().map(|term| term.id));
        }
        log::info!(
            "agents",
            "close agents",
            workspace = plan.place.1,
            tabs = tabs.len(),
            agents = plan.agents.len(),
            run = run.is_some()
        );
        self.agents.closed.keep(plan.place.1, tabs, run.map(|run| KeptRun { run, seats }));
        self.agents.closed.mark_closing(panes);
        self.close_tabs(&plan.tabs);
        self.archive_post(plan.place.1);
    }

    fn closed_tab(&self, tab: &Tab, plan: &Plan, run: Option<&Run>, live: &std::collections::HashSet<u64>) -> TabState {
        let mut saved = TabState::of(tab, live);
        for (state, term) in saved.panes.iter_mut().zip(&tab.panes) {
            if plan.agents.contains(&term.id) {
                state.closed = Some(self.closed_agent(term, plan.cut.contains(&term.id), run));
            }
        }
        saved
    }

    fn closed_agent(&self, term: &Term, cut: bool, run: Option<&Run>) -> ClosedAgent {
        let seat = run.and_then(|run| run.flow.role(run.role_of(term.id)?)).map(|role| role.agent.clone());
        let kind = term
            .agent
            .agent()
            .map(str::to_string)
            .or(seat)
            .or_else(|| term.resume.as_ref().map(|agent| agent.kind.clone()))
            .unwrap_or_default();
        let mode = agents::mode_of(&kind, &term.foreground_args(), &agents::modes(&self.config, &kind));
        ClosedAgent { kind, mode, cut }
    }

    pub(in crate::app) fn close_agents_by_key(&mut self) {
        let Some(workspace) = self.project().and_then(Project::workspace).map(|w| w.id) else { return };
        self.screen.nav = None;
        if self.agents_close_plan(workspace).is_ok() {
            self.ask_close_agents(workspace);
        } else {
            self.messages.toast = Some(Toast::new(NO_AGENT_TO_CLOSE, ui::ToastIcon::Check));
        }
    }

    pub(in crate::app) fn reopen_agents_by_key(&mut self, area: Rect) {
        let Some(workspace) = self.project().and_then(Project::workspace).map(|w| w.id) else { return };
        self.screen.nav = None;
        if self.agents.closed.count(workspace) > 0 {
            self.reopen_from_window(workspace, area);
        } else {
            self.messages.toast = Some(Toast::new(NO_CLOSED_AGENTS, ui::ToastIcon::Check));
        }
    }

    pub(in crate::app) fn forgotten_note(&self, workspace: u64) -> String {
        forgotten(self.agents.closed.count(workspace))
    }

    pub(in crate::app) fn closed_agents_label(&self, action: MenuAction) -> Option<String> {
        Some(match action {
            MenuAction::ReopenAgents(workspace) => {
                format!("reopen {}", agents_label(self.agents.closed.count(workspace)))
            }
            MenuAction::ForgetAgents(_) => "forget closed agents".to_string(),
            MenuAction::CloseAgents(_) => "close agents".to_string(),
            _ => return None,
        })
    }

    pub(in crate::app) fn closed_agents_menu(&self, workspace: u64) -> Vec<MenuAction> {
        let close = self.agents_close_plan(workspace).is_ok().then_some(MenuAction::CloseAgents(workspace));
        let kept = self.agents.closed.count(workspace) > 0;
        let kept = kept.then_some([MenuAction::ReopenAgents(workspace), MenuAction::ForgetAgents(workspace)]);
        close.into_iter().chain(kept.into_iter().flatten()).collect()
    }

    pub(in crate::app) fn reopen_from_window(&mut self, workspace: u64, area: Rect) {
        match self.reopen_agents(workspace, area) {
            Ok(done) => {
                let place = self.projects.workspace_index(workspace).map(|at| self.place_name(at)).unwrap_or_default();
                let message = format!("{} reopened in {place}", agents_label(done.panes.len()));
                self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
            }
            Err(refusal) => self.refused_agents(&refusal),
        }
    }

    pub(in crate::app) fn close_agents_with_toast(&mut self, plan: &Plan) {
        self.close_agents(plan);
        let mut toast = Toast::new(closed_message(plan.agents.len()), ui::ToastIcon::Check);
        toast.undo = Some(Undo::Agents(plan.place.1));
        self.messages.toast = Some(toast);
    }
}

fn seats_in(run: &Run, tab: &Tab, index: usize) -> Vec<SeatState> {
    let found = |member: &crate::flows::run::Member| {
        tab.index_of(member.pane).map(|pane| SeatState { role: member.role.clone(), tab: index, pane })
    };
    run.members.iter().filter_map(found).collect()
}
