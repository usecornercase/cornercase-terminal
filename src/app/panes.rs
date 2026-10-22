use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::interaction::{Drag, LinkPress, PaneDrag};
use super::menus::Marked;
use super::menus::MenuAction;
use super::menus::PaneAction;
use super::opening::Subject;
use super::overlay::Overlay;
use super::plugins::Spot;
use super::toast::NOT_READING;
use super::toast::Toast;
use crate::error::Result;
use crate::log::{self};
use crate::project::split::{self, Dir};
use crate::project::{Hierarchy, Project, Workspace};
use crate::term::{Term, mouse};
use crate::ui::{self};

impl App {
    pub(super) fn pane_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if self.pane_footer_mouse(ev, pos, area)
            || self.pane_notice_mouse(ev, pos, area)
            || self.pane_chosen_mouse(ev, pos, area)
        {
            return;
        }
        self.pane_press_mouse(ev, area);
    }

    pub(super) fn no_tab(&self) -> ui::NoTab {
        let workspace = self.project().and_then(Project::workspace);
        ui::NoTab {
            closed: workspace.map_or(0, |w| self.agents.closed.count(w.id)),
            removing: workspace.is_some_and(Workspace::removing),
        }
    }

    pub(super) fn empty_pane_mouse(
        &mut self,
        kind: MouseEventKind,
        pos: Position,
        pane: Rect,
        area: Rect,
    ) -> Result<()> {
        if kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        if self.project().is_some() {
            return self.press_no_tab(pos, pane, area);
        }
        if self.projects.is_empty() && ui::welcome_button(pane).is_some_and(|button| button.contains(pos)) {
            self.new_project_menu(pos);
        }
        Ok(())
    }

    fn press_no_tab(&mut self, pos: Position, pane: Rect, area: Rect) -> Result<()> {
        let buttons = ui::no_tab_buttons(pane, self.no_tab());
        if buttons.reopen.is_some_and(|button| button.contains(pos))
            && let Some(id) = self.project().and_then(Project::workspace).map(|w| w.id)
        {
            self.reopen_from_window(id, area);
        } else if buttons.new_tab.is_some_and(|button| button.contains(pos)) {
            let at = self.ensure_workspace(self.active);
            return self.add_tab(at, area);
        }
        Ok(())
    }

    pub(super) fn terminals(&self, area: Rect) -> Vec<(u64, Rect)> {
        self.footers(area).into_iter().map(|(id, terminal, _)| (id, terminal)).collect()
    }

    pub(super) fn terminal_rect(&self, area: Rect, pane: u64) -> Option<Rect> {
        self.terminals(area).into_iter().find(|(id, _)| *id == pane).map(|(_, rect)| rect)
    }

    fn footers(&self, area: Rect) -> Vec<(u64, Rect, Option<Rect>)> {
        let Some(tab) = self.tab() else { return Vec::new() };
        let footer = self.tab_footer(tab.id);
        let split = |(id, leaf)| {
            let (terminal, row) = ui::split_footer(leaf, footer);
            (id, terminal, row)
        };
        tab.shown(area).into_iter().map(split).collect()
    }

    fn footer_at(&self, area: Rect, pos: Position) -> Option<u64> {
        let found = self.footers(area).into_iter().find(|(_, _, row)| row.is_some_and(|row| row.contains(pos)));
        found.map(|(id, _, _)| id)
    }

    fn pane_footer_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_)) {
            return false;
        }
        let Some(pane) = self.footer_at(area, pos) else { return false };
        match ev.kind {
            MouseEventKind::Down(MouseButton::Right) => self.start_pane_drag(pane, pos, area),
            MouseEventKind::Down(_) => self.screen.drag = Some(Drag::Footer(pane)),
            _ => {}
        }
        true
    }

    fn start_pane_drag(&mut self, pane: u64, from: Position, area: Rect) {
        let Some(tab) = self.tab() else { return };
        let (rect, tab) = (tab.rect(area, pane), tab.id);
        self.screen.drag = rect.map(|rect| Drag::Pane(PaneDrag { tab, pane, from, rect, moved: false }));
    }

    pub(super) fn drag_footer(&mut self, pane: u64, ev: MouseEvent, area: Rect) {
        if matches!(ev.kind, MouseEventKind::Drag(_)) {
            return;
        }
        self.screen.drag = None;
        if ev.kind == MouseEventKind::Up(MouseButton::Left) {
            self.press_pill(pane, Position::new(ev.column, ev.row), area);
        }
    }

    fn press_pill(&mut self, pane: u64, pos: Position, area: Rect) {
        let row = self.footers(area).into_iter().find_map(|(id, _, row)| row.filter(|_| id == pane));
        let Some((row, term)) = row.zip(self.projects.pane_by(pane)) else { return };
        if let Some(index) = ui::account_pill_hit(row, &self.pane_pills(term).pills, pos) {
            self.screen.nav = None;
            self.open_pill_menu(pane, index, pos);
        }
    }

    fn pane_notice_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        if let Some(delta) = wheel(ev.kind)
            && self.wheel_pane(area, pos, delta)
        {
            return true;
        }
        ev.kind == MouseEventKind::Down(MouseButton::Left)
            && (self.press_resume_notice(area, pos)
                || self.press_held_notice(area, pos)
                || self.press_scrollback_badge(area, pos))
    }

    fn pane_chosen_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        let Some(tab) = self.tab() else { return true };
        let continues_inside = matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !continues_inside && let Some(divider) = tab.layout.divider_at(area, pos) {
            if left {
                self.press_divider(tab.id, divider.path, Instant::now());
            }
            return true;
        }
        let Some(active) = tab.pane().map(|t| t.id) else { return true };
        if continues_inside {
            return false;
        }
        let Some(id) = tab.pane_at(area, pos) else { return true };
        let program_takes_right = tab.right_clicks_to_pane(id)
            && tab.pane_by(id).is_some_and(|t| t.emulator.mouse_mode() != mouse::MouseMode::None);
        if right && !program_takes_right {
            self.start_pane_drag(id, pos, area);
            return true;
        }
        if id == active {
            return false;
        }
        if (left || right)
            && let Some(t) = self.tab_mut()
        {
            t.focus(id);
        }
        !right
    }

    fn pane_press_mouse(&mut self, ev: MouseEvent, area: Rect) {
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let Some(term) = self.term() else { return };
        let Some(pane) = self.terminal_rect(area, term.id).map(|rect| within_terminal(term, rect)) else {
            return;
        };
        let Some(at) = pane_cell(pane, ev) else { return };
        if self.link_click(ev, at) {
            return;
        }
        let Some(term) = self.term_mut() else { return };
        if term.emulator.mouse_mode() == mouse::MouseMode::None && left {
            let id = term.id;
            if term.emulator.start_selection(at).is_ok() {
                let link = self.screen.drag.take().and_then(|drag| drag.link().cloned());
                self.screen.drag = Some(Drag::Selection { term: id, link });
            }
            return;
        }
        self.forward_mouse(ev, at);
    }

    pub(super) fn forward_mouse(&mut self, ev: MouseEvent, at: Position) {
        if self.term().is_some_and(|term| self.resume_holds(term.id) || self.sending(term.id)) {
            return;
        }
        let Some(term) = self.term_mut() else { return };
        let bytes = mouse::encode(ev, at.x, at.y, term.emulator.mouse_mode(), term.emulator.mouse_encoding());
        if let Some(bytes) = bytes {
            term.write(&bytes);
        }
    }

    fn wheel_pane(&mut self, area: Rect, pos: Position, delta: isize) -> bool {
        let held = self
            .tab()
            .and_then(|tab| tab.pane_at(area, pos))
            .is_some_and(|id| self.resume_holds(id) || self.sending(id));
        let Some(tab) = self.tab_mut() else { return true };
        let Some(id) = tab.pane_at(area, pos) else { return true };
        let Some(term) = tab.pane_by_mut(id) else { return true };
        if term.emulator.mouse_mode() != mouse::MouseMode::None {
            return false;
        }
        let refused = if !term.emulator.alternate_screen() {
            term.emulator.scroll(delta);
            false
        } else if held {
            false
        } else if term.emulator.alt_scroll() {
            let code = if delta < 0 { KeyCode::Up } else { KeyCode::Down };
            let key = term.emulator.encode_key(KeyEvent::new(code, KeyModifiers::NONE));
            !term.write(&key.repeat(delta.unsigned_abs()))
        } else {
            false
        };
        if refused {
            self.messages.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
        true
    }

    fn press_scrollback_badge(&mut self, area: Rect, pos: Position) -> bool {
        let Some((id, rect)) = self.terminals(area).into_iter().find(|(_, r)| r.contains(pos)) else { return false };
        let Some(term) = self.tab_mut().and_then(|tab| tab.pane_by_mut(id)) else { return false };
        if !ui::scrollback_badge(rect, term.emulator.below()).is_some_and(|badge| badge.contains(pos)) {
            return false;
        }
        term.emulator.scroll_to_bottom();
        true
    }

    pub(super) fn drag_selection(&mut self, id: u64, mut link: Option<LinkPress>, ev: MouseEvent, pane: Rect) {
        let Some(term) = self.projects.pane_by_mut(id) else {
            self.screen.drag = None;
            return;
        };
        let at = pane_cell(within_terminal(term, pane), ev);
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            if link.as_ref().is_some_and(|p| Some(p.at) != at) {
                link = None;
            }
            if let Some(at) = at
                && term.emulator.extend_selection(at).is_err()
            {
                self.screen.drag = None;
            } else {
                self.screen.drag = Some(Drag::Selection { term: id, link });
            }
            return;
        }
        self.screen.drag = None;
        if let Ok(Some(text)) = term.emulator.finish_selection() {
            self.copy(&text);
        }
        if let Some(press) = link.filter(|p| p.term == id && Some(p.at) == at) {
            self.open_link(&press.target);
        }
    }

    fn press_divider(&mut self, tab: u64, path: Vec<bool>, now: Instant) {
        if self.screen.clicks.divider_pressed(tab, &path, now) {
            if let Some(t) = self.tab_mut().filter(|t| t.id == tab) {
                t.layout.set_ratio(&path, split::HALF);
            }
        } else {
            self.screen.drag = Some(Drag::Divider { tab, path });
        }
    }

    pub(super) fn drag_divider(&mut self, tab: u64, path: &[bool], ev: MouseEvent, pane: Rect) {
        if ev.kind != MouseEventKind::Drag(MouseButton::Left) {
            self.screen.drag = None;
            return;
        }
        self.screen.clicks.forget_divider();
        let Some(t) = self.tab_mut().filter(|t| t.id == tab) else { return };
        if let Some(divider) = t.layout.dividers(pane).into_iter().find(|d| d.path == path) {
            t.layout.set_ratio(path, split::ratio_at(&divider, Position::new(ev.column, ev.row)));
        }
    }

    pub(super) fn drag_pane(&mut self, drag: PaneDrag, ev: MouseEvent, pane: Rect, area: Rect) {
        let pos = Position::new(ev.column, ev.row);
        let moved = drag.moved || !drag.rect.contains(pos);
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Right) => self.screen.drag = Some(Drag::Pane(PaneDrag { moved, ..drag })),
            MouseEventKind::Up(_) => {
                self.screen.drag = None;
                if moved {
                    self.drop_pane(drag, pos, pane, area);
                } else {
                    self.open_pane_menu(drag.pane, drag.from, pane);
                }
            }
            _ => self.screen.drag = None,
        }
    }

    fn drop_pane(&mut self, drag: PaneDrag, pos: Position, pane: Rect, area: Rect) {
        let Some(tab) = self.tab_mut().filter(|t| t.id == drag.tab) else { return };
        let Some(landing) = tab.landing(pane, drag.pane, pos) else { return };
        log::info!("app", "pane moved", pane = drag.pane, tab = drag.tab, place = format!("{:?}", landing.place));
        tab.land(drag.pane, landing.layout);
        self.resize(area);
    }

    pub(super) fn pane_landing(&self, pane: Rect) -> Option<(Rect, split::Place)> {
        let drag = match &self.screen.drag {
            Some(Drag::Pane(drag)) if drag.moved => *drag,
            Some(
                Drag::Border(_)
                | Drag::Divider { .. }
                | Drag::Row(_)
                | Drag::Pane(_)
                | Drag::Selection { .. }
                | Drag::Link(_)
                | Drag::Lines { .. }
                | Drag::Footer(_),
            )
            | None => return None,
        };
        let tab = self.tab().filter(|t| t.id == drag.tab)?;
        tab.landing(pane, drag.pane, self.screen.hover?).map(|l| (l.area, l.place))
    }

    fn open_pane_menu(&mut self, pane: u64, at: Position, area: Rect) {
        let Some(tab) = self.tab() else { return };
        if tab.rect(area, pane).is_none() {
            return;
        }
        let right_clicks =
            if tab.right_clicks_to_pane(pane) { PaneAction::RightClicksToMenu } else { PaneAction::RightClicksToPane };
        let mut actions: Vec<MenuAction> = [Dir::Right, Dir::Down]
            .into_iter()
            .filter(|&d| tab.can_split(area, pane, d))
            .map(PaneAction::Split)
            .chain([right_clicks])
            .map(|a| MenuAction::Pane(pane, a))
            .collect();
        actions.extend(self.mark_action(Marked::Pane(pane)));
        actions.extend(self.account_entry(Some(pane)));
        actions.push(self.open_entry(Subject::Pane(pane)));
        actions.push(MenuAction::Pane(pane, PaneAction::Close));
        actions.extend(self.plugin_menu(Spot::Pane(pane)));
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn pane_action(&mut self, pane: u64, action: PaneAction, area: Rect) -> Result<()> {
        let Some((_, tab)) = self.projects.tab_with_pane_mut(pane) else { return Ok(()) };
        match action {
            PaneAction::Split(dir) => return self.split_pane(pane, dir, area, true).map(drop),
            PaneAction::RightClicksToPane | PaneAction::RightClicksToMenu => tab.toggle_right_clicks(pane),
            PaneAction::Close => {
                if let Some(term) = tab.pane_by_mut(pane) {
                    term.kill();
                }
            }
        }
        Ok(())
    }
}

pub(super) fn within_terminal(term: &Term, rect: Rect) -> Rect {
    let Ok((rows, cols)) = term.emulator.size() else { return rect };
    Rect::new(rect.x, rect.y, rect.width.min(cols.max(1)), rect.height.min(rows.max(1)))
}

pub(super) fn pane_cell(pane: Rect, ev: MouseEvent) -> Option<Position> {
    if pane.is_empty() {
        return None;
    }
    Some(Position::new(
        ev.column.clamp(pane.x, pane.right() - 1) - pane.x,
        ev.row.clamp(pane.y, pane.bottom() - 1) - pane.y,
    ))
}
