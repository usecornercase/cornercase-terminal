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
use crate::project::{Hierarchy, WorkspaceAt};
use crate::ui;

mod check;
mod message;

pub(crate) use check::Findings;
pub(in crate::app) use check::MAY_BE_LOST;

pub(super) const CLOSE_AND_REMOVE_SUBMIT: &str = "close and remove";

const TITLE: &str = "Close agent";

const CHECKING: &str = "checking…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Plan {
    pub(in crate::app) pane: u64,
    pub(in crate::app) place: (u64, u64),
    pub(in crate::app) remove: Option<(u64, u64)>,
    pub(in crate::app) closing: Closing,
    pub(in crate::app) stops: Vec<u64>,
    pub(in crate::app) run: Option<u64>,
}

impl Plan {
    pub(in crate::app) fn grew(&self, shown: &Self) -> bool {
        let new = |now: &[u64], then: &[u64]| now.iter().any(|id| !then.contains(id));
        self.remove != shown.remove
            || self.run != shown.run
            || new(&self.closing.tabs, &shown.closing.tabs)
            || new(&self.stops, &shown.stops)
    }
}

#[derive(Debug)]
pub(super) struct CloseAgent {
    pub(super) plan: Plan,
    pub(super) found: Option<Findings>,
    pub(super) shown: Option<Findings>,
    pub(super) generation: u64,
    pub(super) branches: bool,
    pub(super) pressed: bool,
    pub(super) scroll: usize,
}

impl CloseAgent {
    fn ready(&self) -> bool {
        self.found.is_some()
    }

    fn findings(&self) -> Option<&Findings> {
        self.found.as_ref().or(self.shown.as_ref())
    }

    pub(super) fn submit_label(&self) -> &'static str {
        if self.plan.remove.is_some() { CLOSE_AND_REMOVE_SUBMIT } else { CLOSE_SUBMIT }
    }
}

impl App {
    pub(in crate::app) fn agent_close_plan(&self, pane: u64) -> Result<Plan, Refusal> {
        let at = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?.workspace();
        if self.projects.pane_by(pane).and_then(|term| term.agent.status()).is_none() {
            return Err(conflict(format!("pane {pane} runs no agent cornercase reads")));
        }
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        if project.closing {
            return Err(conflict(format!("project {} is closing", project.id)));
        }
        self.usable_workspace(workspace.id)?;
        let remove = self.removes(at).then_some((project.id, workspace.id));
        let panes = self.closing_panes();
        let mut closing = closing::closing(&panes, pane, remove.is_some()).ok_or_else(|| none("pane", pane))?;
        let roles = if remove.is_some() { self.role_rows(at) } else { Vec::new() };
        let in_roles = |id: &u64| panes.iter().any(|pane| pane.id == *id && roles.contains(&pane.workspace));
        closing.elsewhere.retain(|id| !in_roles(id));
        let stopping = |pane: &&Pane| closing.tabs.contains(&pane.tab) || roles.contains(&pane.workspace);
        let stops = panes.iter().filter(stopping).map(|pane| pane.id).collect();
        let run = self.run_of(workspace.id).map(|run| run.id);
        Ok(Plan { pane, place: (project.id, workspace.id), remove, closing, stops, run })
    }

    fn removes(&self, at: WorkspaceAt) -> bool {
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        let linked = workspace.worktree && workspace.path != project.path && workspace.path.join(".git").is_file();
        linked && self.role_owner(at).is_none()
    }

    fn role_rows(&self, at: WorkspaceAt) -> Vec<u64> {
        let project = &self.projects[at.project()];
        let trees = &self.projects.workspace(at).role_worktrees;
        let rows = project.workspaces.iter().filter(|row| row.open() && trees.iter().any(|tree| tree.path == row.path));
        rows.map(|row| row.id).collect()
    }

    fn closing_panes(&self) -> Vec<Pane> {
        let tabs = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| w.tabs.iter().map(move |t| (w.id, t)));
        tabs.flat_map(|(workspace, tab)| {
            tab.panes.iter().map(move |term| Pane {
                id: term.id,
                parent: term.parent,
                workspace,
                tab: tab.id,
                agent: term.agent.status().is_some(),
            })
        })
        .collect()
    }

    pub(super) fn ask_close_agent(&mut self, pane: u64) {
        match self.agent_close_plan(pane) {
            Ok(plan) => self.open_close_agent(plan, false),
            Err(refusal) => self.refused_close(&refusal),
        }
    }

    fn refused_close(&mut self, refusal: &Refusal) {
        if refusal.code != Code::NotFound {
            self.messages.toast = Some(Toast::new(refusal.message.clone(), ui::ToastIcon::Check));
        }
    }

    fn open_close_agent(&mut self, plan: Plan, branches: bool) {
        let place = plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let generation = place.map_or(0, |place| self.spawn_close_check(place));
        let found = place.is_none().then(Findings::default);
        let dialog = CloseAgent { plan, found, shown: None, generation, branches, pressed: branches, scroll: 0 };
        self.overlay = Some(Overlay::CloseAgent(Box::new(dialog)));
    }

    pub(super) fn confirm_close_agent(&mut self) {
        let Some(Overlay::CloseAgent(dialog)) = self.overlay.take() else { return };
        if !dialog.ready() {
            self.overlay = Some(Overlay::CloseAgent(dialog));
            return;
        }
        self.go_on_closing(*dialog);
    }

    fn go_on_closing(&mut self, dialog: CloseAgent) {
        match self.agent_close_plan(dialog.plan.pane) {
            Err(refusal) => self.refused_close(&refusal),
            Ok(plan) if plan.grew(&dialog.plan) => self.open_close_agent(plan, dialog.branches),
            Ok(plan) if plan.remove.is_some() && dialog.shown.is_none() => self.check_again(plan, dialog),
            Ok(plan) => self.close_agent(&plan, &dialog),
        }
    }

    fn check_again(&mut self, plan: Plan, mut dialog: CloseAgent) {
        let place = plan.remove.and_then(|(project, workspace)| self.projects.workspace_in(project, workspace));
        let Some(place) = place else { return };
        dialog.shown = dialog.found.take();
        dialog.pressed = dialog.branches;
        dialog.plan = plan;
        dialog.generation = self.spawn_close_check(place);
        self.overlay = Some(Overlay::CloseAgent(Box::new(dialog)));
    }

    fn close_agent_answered(&mut self) {
        let Some(Overlay::CloseAgent(dialog)) = &mut self.overlay else { return };
        let (Some(found), Some(shown)) = (&dialog.found, &dialog.shown) else { return };
        if dialog.branches != dialog.pressed || found.riskier_than(shown, dialog.branches) {
            dialog.shown = None;
            return;
        }
        let Some(Overlay::CloseAgent(dialog)) = self.overlay.take() else { return };
        self.go_on_closing(*dialog);
    }

    fn close_agent(&mut self, plan: &Plan, dialog: &CloseAgent) {
        log::info!(
            "agents",
            "close agent",
            pane = plan.pane,
            remove = plan.remove.is_some(),
            tabs = plan.closing.tabs.len(),
            branches = dialog.branches
        );
        let Some((project, workspace)) = plan.remove else {
            self.close_tabs(&plan.closing.tabs);
            return;
        };
        let (Some(at), Some(found)) = (self.projects.workspace_in(project, workspace), &dialog.found) else {
            return;
        };
        let shown = found.shown(dialog.branches);
        if !self.remove_with_shown_branches(at, shown, (found.changed, found.lock.is_some()), None) {
            self.messages.toast = Some(Toast::new("the workspace could not be removed", ui::ToastIcon::Bug));
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
            KeyCode::Char(' ') => self.toggle_close_branches(),
            KeyCode::Up => self.scroll_close_agent(-1, area),
            KeyCode::Down => self.scroll_close_agent(1, area),
            _ => {}
        }
    }

    pub(super) fn close_agent_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::CloseAgent(dialog)) = &self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_close_agent(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, dialog.submit_label(), ui::CANCEL_LABEL);
        if submit.contains(pos) {
            self.confirm_close_agent();
        } else if cancel.contains(pos) {
            self.overlay = None;
        } else if ui::update_toggle(area).contains(pos) {
            self.toggle_close_branches();
        }
    }

    fn toggle_close_branches(&mut self) {
        if let Some(Overlay::CloseAgent(dialog)) = &mut self.overlay
            && dialog.plan.remove.is_some()
            && dialog.ready()
        {
            dialog.branches = !dialog.branches;
        }
    }

    fn scroll_close_agent(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::CloseAgent(dialog)) = &self.overlay else { return };
        let lines = self.close_agent_notes(dialog, area).len();
        let body = ui::update_body(area, dialog.plan.remove.is_some());
        let scroll = ui::scroll_in(body, lines, dialog.scroll.saturating_add_signed(delta));
        if let Some(Overlay::CloseAgent(dialog)) = &mut self.overlay {
            dialog.scroll = scroll;
        }
    }

    pub(super) fn close_agent_view(&self, dialog: &CloseAgent, area: Rect) -> ui::Overlay {
        let toggle = dialog.plan.remove.map(|_| ui::Toggle { label: ui::BRANCHES_LABEL, on: dialog.branches });
        let notes = self.close_agent_notes(dialog, area);
        let scroll = ui::scroll_in(ui::update_body(area, toggle.is_some()), notes.len(), dialog.scroll);
        let ready = dialog.ready();
        ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: self.close_agent_heading(&dialog.plan),
            notes,
            scroll,
            note: (!ready).then_some(ui::Note::Busy(CHECKING)),
            submit: dialog.submit_label(),
            extra: None,
            cancel: ui::CANCEL_LABEL,
            toggle,
            waiting: !ready,
        })
    }
}
