use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::control::{conflict, none};
use super::input::wheel;
use super::overlay::{CLOSE_SUBMIT, Overlay};
use super::toast::Toast;
use crate::agents::closing::{self, Closing, Pane};
use crate::control::contract::{Code, Refusal};
use crate::log::{self};
use crate::project::Hierarchy;
use crate::ui;

mod message;

pub(in crate::app) use message::section;

const TITLE: &str = "Close agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Plan {
    pub(in crate::app) pane: u64,
    pub(in crate::app) place: (u64, u64),
    pub(in crate::app) closing: Closing,
    pub(in crate::app) stops: Vec<u64>,
    pub(in crate::app) run: Option<u64>,
}

impl Plan {
    pub(in crate::app) fn grew(&self, shown: &Self) -> bool {
        let new = |now: &[u64], then: &[u64]| now.iter().any(|id| !then.contains(id));
        self.run != shown.run || new(&self.closing.tabs, &shown.closing.tabs) || new(&self.stops, &shown.stops)
    }
}

#[derive(Debug)]
pub(super) struct CloseAgent {
    pub(super) plan: Plan,
    pub(super) scroll: usize,
    pub(super) stop: bool,
}

impl App {
    pub(in crate::app) fn agent_close_plan(&self, pane: u64) -> Result<Plan, Refusal> {
        self.close_plan(pane, true)
    }

    fn close_plan(&self, pane: u64, agent_needed: bool) -> Result<Plan, Refusal> {
        let at = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?.workspace();
        if agent_needed && self.projects.pane_by(pane).and_then(|term| term.agent.status()).is_none() {
            return Err(conflict(format!("pane {pane} runs no agent cornercase reads")));
        }
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        if project.closing {
            return Err(conflict(format!("project {} is closing", project.id)));
        }
        self.usable_workspace(workspace.id)?;
        let run = self.run_of(workspace.id);
        let team: Vec<u64> = run.iter().flat_map(|run| run.members.iter().map(|member| member.pane)).collect();
        let panes = self.closing_panes();
        let closing = closing::closing(&panes, pane, &team).ok_or_else(|| none("pane", pane))?;
        let stops: Vec<u64> =
            panes.iter().filter(|other| closing.tabs.contains(&other.tab)).map(|other| other.id).collect();
        let run = run.filter(|_| team.iter().any(|member| stops.contains(member))).map(|run| run.id);
        Ok(Plan { pane, place: (project.id, workspace.id), closing, stops, run })
    }

    fn closing_panes(&self) -> Vec<Pane> {
        let tabs = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| w.tabs.iter().map(move |t| (w.id, t)));
        tabs.flat_map(|(workspace, tab)| {
            tab.panes.iter().map(move |term| Pane { id: term.id, parent: term.parent, workspace, tab: tab.id })
        })
        .collect()
    }

    pub(super) fn ask_close_agent(&mut self, pane: u64) {
        match self.agent_close_plan(pane) {
            Ok(plan) => self.open_close_agent(plan),
            Err(refusal) => self.refused_close(&refusal),
        }
    }

    pub(super) fn ask_stop_run(&mut self, pane: u64) {
        match self.close_plan(pane, false) {
            Ok(plan) => self.overlay = Some(Overlay::CloseAgent(Box::new(CloseAgent { plan, scroll: 0, stop: true }))),
            Err(refusal) => self.refused_close(&refusal),
        }
    }

    fn refused_close(&mut self, refusal: &Refusal) {
        if refusal.code != Code::NotFound {
            self.messages.toast = Some(Toast::new(refusal.message.clone(), ui::ToastIcon::Check));
        }
    }

    fn open_close_agent(&mut self, plan: Plan) {
        self.overlay = Some(Overlay::CloseAgent(Box::new(CloseAgent { plan, scroll: 0, stop: false })));
    }

    pub(super) fn confirm_close_agent(&mut self) {
        let Some(Overlay::CloseAgent(dialog)) = self.overlay.take() else { return };
        match self.close_plan(dialog.plan.pane, !dialog.stop) {
            Err(refusal) => self.refused_close(&refusal),
            Ok(plan) if plan.grew(&dialog.plan) => {
                self.overlay = Some(Overlay::CloseAgent(Box::new(CloseAgent { plan, scroll: 0, stop: dialog.stop })));
            }
            Ok(plan) => {
                log::info!(
                    "agents",
                    "close agent",
                    pane = plan.pane,
                    tabs = plan.closing.tabs.len(),
                    run = plan.run.is_some()
                );
                self.close_tabs(&plan.closing.tabs);
            }
        }
    }

    pub(in crate::app) fn close_tabs(&mut self, tabs: &[u64]) {
        for &tab in tabs {
            if let Some(at) = self.projects.tab_index(tab) {
                self.close_tab(at);
            }
        }
    }

    pub(super) fn close_agent_key(&mut self, key: KeyEvent, area: Rect) {
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.confirm_close_agent(),
            KeyCode::Up => self.scroll_close_agent(-1, area),
            KeyCode::Down => self.scroll_close_agent(1, area),
            _ => {}
        }
    }

    pub(super) fn close_agent_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_close_agent(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, CLOSE_SUBMIT, ui::CANCEL_LABEL);
        if submit.contains(pos) {
            self.confirm_close_agent();
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    fn scroll_close_agent(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::CloseAgent(dialog)) = &self.overlay else { return };
        let lines = self.close_agent_notes(&dialog.plan, area).len();
        let scroll = ui::update_scroll(area, lines, dialog.scroll.saturating_add_signed(delta));
        if let Some(Overlay::CloseAgent(dialog)) = &mut self.overlay {
            dialog.scroll = scroll;
        }
    }

    pub(super) fn close_agent_view(&self, dialog: &CloseAgent, area: Rect) -> ui::Overlay {
        let notes = self.close_agent_notes(&dialog.plan, area);
        let scroll = ui::update_scroll(area, notes.len(), dialog.scroll);
        ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: self.close_agent_heading(&dialog.plan),
            notes,
            scroll,
            note: None,
            submit: CLOSE_SUBMIT,
            extra: None,
            cancel: ui::CANCEL_LABEL,
        })
    }
}
