use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use super::{Plan, agents_label};
use crate::app::App;
use crate::app::close_agent::section;
use crate::app::control::conflict;
use crate::app::dialogs::confirm_view;
use crate::app::input::wheel;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::app::worktrees_dialog::wrap;
use crate::control::contract::{Code, Refusal};
use crate::project::{Hierarchy, Tab};
use crate::term::Term;
use crate::ui;

const TITLE: &str = "Close agents";

pub(in crate::app) const CLOSE_AGENTS_SUBMIT: &str = "close agents";

pub(in crate::app) const FORGET_SUBMIT: &str = "forget";

#[derive(Debug)]
pub(in crate::app) struct CloseAgents {
    pub(in crate::app) plan: Plan,
    pub(in crate::app) scroll: usize,
}

fn names(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl App {
    pub(in crate::app) fn ask_close_agents(&mut self, workspace: u64) {
        match self.agents_close_plan(workspace) {
            Ok(plan) if !plan.asks() => self.close_agents_with_toast(&plan),
            Ok(plan) => self.overlay = Some(Overlay::CloseAgents(Box::new(CloseAgents { plan, scroll: 0 }))),
            Err(refusal) => self.refused_agents(&refusal),
        }
    }

    pub(in crate::app) fn refused_agents(&mut self, refusal: &Refusal) {
        let icon = if refusal.code == Code::Failed { ui::ToastIcon::Bug } else { ui::ToastIcon::Check };
        if refusal.code != Code::NotFound {
            self.messages.toast = Some(Toast::new(refusal.message.clone(), icon));
        }
    }

    pub(in crate::app) fn confirm_close_agents(&mut self) {
        let Some(Overlay::CloseAgents(dialog)) = self.overlay.take() else { return };
        match self.agents_close_plan(dialog.plan.place.1) {
            Err(refusal) => self.refused_agents(&refusal),
            Ok(plan) if plan.grew(&dialog.plan) => {
                self.overlay = Some(Overlay::CloseAgents(Box::new(CloseAgents { plan, scroll: 0 })));
            }
            Ok(plan) => self.close_agents_with_toast(&plan),
        }
    }

    pub(in crate::app) fn close_agents_key(&mut self, key: KeyEvent, area: Rect) {
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.confirm_close_agents(),
            KeyCode::Up => self.scroll_close_agents(-1, area),
            KeyCode::Down => self.scroll_close_agents(1, area),
            _ => {}
        }
    }

    pub(in crate::app) fn close_agents_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_close_agents(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, CLOSE_AGENTS_SUBMIT, ui::CANCEL_LABEL);
        if submit.contains(pos) {
            self.confirm_close_agents();
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    fn scroll_close_agents(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::CloseAgents(dialog)) = &self.overlay else { return };
        let lines = self.close_agents_notes(dialog, area).len();
        let scroll = ui::update_scroll(area, lines, dialog.scroll.saturating_add_signed(delta));
        if let Some(Overlay::CloseAgents(dialog)) = &mut self.overlay {
            dialog.scroll = scroll;
        }
    }

    pub(in crate::app) fn closed_agents_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        match overlay {
            Overlay::CloseAgents(dialog) => Some(self.close_agents_view(dialog, area)),
            Overlay::ForgetAgents { workspace, .. } => self.forget_agents_view(*workspace),
            _ => None,
        }
    }

    fn close_agents_view(&self, dialog: &CloseAgents, area: Rect) -> ui::Overlay {
        let notes = self.close_agents_notes(dialog, area);
        let scroll = ui::update_scroll(area, notes.len(), dialog.scroll);
        let place = self.closing_place(dialog.plan.place);
        let message = format!(
            "Close {} in {place}? {} on the workspace reopens {}, each in its own conversation.",
            agents_label(dialog.plan.agents.len()),
            ui::REOPEN_ICON,
            if dialog.plan.agents.len() == 1 { "it" } else { "them" },
        );
        ui::Overlay::Update(ui::Update {
            title: TITLE,
            message,
            notes,
            scroll,
            note: None,
            submit: CLOSE_AGENTS_SUBMIT,
            extra: None,
            cancel: ui::CANCEL_LABEL,
        })
    }

    fn close_agents_notes(&self, dialog: &CloseAgents, area: Rect) -> Vec<Line<'static>> {
        let plan = &dialog.plan;
        let name = |id: &u64| self.closed_name(plan, *id);
        let cut: Vec<String> = plan.cut.iter().map(name).collect();
        let fresh: Vec<String> = plan.fresh.iter().map(name).collect();
        let mut lines = Vec::new();
        if !cut.is_empty() {
            let (verb, turn) = if cut.len() == 1 { ("is", "its") } else { ("are", "their") };
            lines.push(Line::from(format!("{} {verb} working: closing cuts {turn} turn.", names(&cut))));
        }
        if !fresh.is_empty() {
            let (verb, what) =
                if fresh.len() == 1 { ("starts", "a new conversation") } else { ("start", "new conversations") };
            lines.push(Line::from(format!("{} {verb} {what}.", names(&fresh))));
        }
        lines.extend(section("Also stops:", self.closing_others(plan)));
        wrap(lines, ui::update_notes(area).width)
    }

    fn closed_name(&self, plan: &Plan, pane: u64) -> String {
        let role = self.run_of(plan.place.1).and_then(|run| run.role_of(pane));
        let term = self.projects.pane_by(pane);
        role.map(str::to_string).or_else(|| term.map(|term| self.closing_agent_name(term))).unwrap_or_default()
    }

    fn closing_others(&self, plan: &Plan) -> Vec<String> {
        let tabs = plan.tabs.iter().filter_map(|&id| self.projects.tab_index(id)).map(|at| self.projects.tab(at));
        tabs.flat_map(|tab| self.tab_others(plan, tab)).collect()
    }

    fn tab_others(&self, plan: &Plan, tab: &Tab) -> Vec<String> {
        let others: Vec<&Term> = tab.panes.iter().filter(|term| plan.others.contains(&term.id)).collect();
        others
            .iter()
            .map(|term| format!("{}, in the tab {}", self.closing_pane(term), tab.label(&self.config)))
            .collect()
    }

    pub(in crate::app) fn ask_forget_agents(&mut self, workspace: u64) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        if self.agents.closed.of(workspace).is_none() {
            let refusal = conflict(format!("workspace {workspace} has no closed agents"));
            return self.refused_agents(&refusal);
        }
        let project = self.projects[at.project()].id;
        self.overlay = Some(Overlay::ForgetAgents { project, workspace });
    }

    pub(in crate::app) fn forget_agents_view(&self, workspace: u64) -> Option<ui::Overlay> {
        let at = self.projects.workspace_index(workspace)?;
        let kept = self.agents.closed.of(workspace)?;
        let count = kept.count();
        let ended = kept.run.as_ref().map_or_else(String::new, |kept| {
            let name = if kept.run.flow.name.is_empty() { String::new() } else { format!(" {}", kept.run.flow.name) };
            format!(" The flow run{name} ends.")
        });
        let message = format!(
            "Forget the {} of {}? They can no longer be reopened here; their conversations stay in each agent's own \
             history.{ended}",
            agents_label(count).replacen(" agent", " closed agent", 1),
            self.place_name(at),
        );
        Some(confirm_view("Forget closed agents", message, FORGET_SUBMIT, None))
    }

    pub(in crate::app) fn submit_forget_agents(&mut self, workspace: u64) -> Option<Overlay> {
        self.forget_agents(workspace);
        None
    }

    pub(in crate::app) fn forget_agents(&mut self, workspace: u64) -> Option<(usize, Option<u64>)> {
        let kept = self.agents.closed.take(workspace)?;
        let run = kept.run.as_ref().map(|kept| kept.run.id);
        if let Some(kept) = &kept.run {
            crate::log::info!("flow", "run ended", workspace = workspace, flow = kept.run.flow.name);
        }
        crate::log::info!("agents", "forget closed agents", workspace = workspace, agents = kept.count());
        Some((kept.count(), run))
    }
}
