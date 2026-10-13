use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::interaction::{DOUBLE_CLICK, Drag, LinkPress, PaneDrag};
use super::menus::MenuAction;
use super::menus::PaneAction;
use super::overlay::Overlay;
use super::toast::NOT_READING;
use super::toast::Toast;
use super::toast::copy;
use crate::error::Result;
use crate::log::{self};
use crate::mouse;
use crate::project::Project;
use crate::split::{self, Dir};
use crate::ui::{self};

impl App {
    pub(super) fn pane_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if let Some(delta) = wheel(ev.kind)
            && self.wheel_pane(area, pos, delta)
        {
            return;
        }
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.press_scrollback_badge(area, pos) {
            return;
        }
        let left = ev.kind == MouseEventKind::Down(MouseButton::Left);
        let right = ev.kind == MouseEventKind::Down(MouseButton::Right);
        let Some(tab) = self.tab() else { return };
        let continues_inside = matches!(ev.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !continues_inside && let Some(divider) = tab.layout.divider_at(area, pos) {
            if left {
                self.press_divider(tab.id, divider.path, Instant::now());
            }
            return;
        }
        let Some(active) = tab.pane().map(|t| t.id) else { return };
        let under = tab.pane_at(area, pos);
        if !continues_inside {
            let Some(id) = under else { return };
            let program_takes_right = tab.right_clicks_to_pane(id)
                && tab.panes.iter().any(|t| t.id == id && t.emulator.mouse_mode() != mouse::MouseMode::None);
            if right && !program_takes_right {
                let rect = tab.rect(area, id);
                let tab = tab.id;
                self.drag = rect.map(|rect| Drag::Pane(PaneDrag { tab, pane: id, from: pos, rect, moved: false }));
                return;
            }
            if id != active {
                if (left || right)
                    && let Some(t) = self.tab_mut()
                {
                    t.focus(id);
                }
                if !right {
                    return;
                }
            }
        }
        let Some(tab) = self.tab() else { return };
        let Some(pane) = tab.pane().and_then(|t| tab.rect(area, t.id)) else { return };
        let Some(at) = pane_cell(pane, ev) else { return };
        if self.link_click(ev, at) {
            return;
        }
        let Some(term) = self.term_mut() else { return };
        if term.emulator.mouse_mode() == mouse::MouseMode::None && left {
            let id = term.id;
            if term.emulator.start_selection(at).is_ok() {
                let link = match self.drag.take() {
                    Some(Drag::Link(press)) => Some(press),
                    Some(
                        Drag::Border(_)
                        | Drag::Divider { .. }
                        | Drag::Row(_)
                        | Drag::Pane(_)
                        | Drag::Selection { .. }
                        | Drag::Lines { .. },
                    )
                    | None => None,
                };
                self.drag = Some(Drag::Selection { term: id, link });
            }
            return;
        }
        self.forward_mouse(ev, at);
    }

    pub(super) fn forward_mouse(&mut self, ev: MouseEvent, at: Position) {
        let Some(term) = self.term_mut() else { return };
        let bytes = mouse::encode(&ev, at.x, at.y, term.emulator.mouse_mode(), term.emulator.mouse_encoding());
        if let Some(bytes) = bytes {
            term.write(&bytes);
        }
    }

    fn wheel_pane(&mut self, area: Rect, pos: Position, delta: isize) -> bool {
        let Some(tab) = self.tab_mut() else { return true };
        let Some(id) = tab.pane_at(area, pos) else { return true };
        let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) else { return true };
        if term.emulator.mouse_mode() != mouse::MouseMode::None {
            return false;
        }
        let refused = if !term.emulator.alternate_screen() {
            term.emulator.scroll(delta);
            false
        } else if term.emulator.alt_scroll() {
            let code = if delta < 0 { KeyCode::Up } else { KeyCode::Down };
            let key = term.emulator.encode_key(KeyEvent::new(code, KeyModifiers::NONE));
            !term.write(&key.repeat(delta.unsigned_abs()))
        } else {
            false
        };
        if refused {
            self.toast = Some(Toast::new(NOT_READING, ui::ToastIcon::Bug));
        }
        true
    }

    fn press_scrollback_badge(&mut self, area: Rect, pos: Position) -> bool {
        let Some(tab) = self.tab_mut() else { return false };
        let Some((id, rect)) = tab.shown(area).into_iter().find(|(_, r)| r.contains(pos)) else { return false };
        let Some(term) = tab.panes.iter_mut().find(|t| t.id == id) else { return false };
        if !ui::scrollback_badge(rect, term.emulator.below()).is_some_and(|badge| badge.contains(pos)) {
            return false;
        }
        term.emulator.scroll_to_bottom();
        true
    }

    pub(super) fn drag_selection(&mut self, id: u64, mut link: Option<LinkPress>, ev: MouseEvent, pane: Rect) {
        let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) else {
            self.drag = None;
            return;
        };
        let at = pane_cell(pane, ev);
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            if link.as_ref().is_some_and(|p| Some(p.at) != at) {
                link = None;
            }
            if let Some(at) = at
                && term.emulator.extend_selection(at).is_err()
            {
                self.drag = None;
            } else {
                self.drag = Some(Drag::Selection { term: id, link });
            }
            return;
        }
        self.drag = None;
        if let Ok(Some(text)) = term.emulator.finish_selection() {
            copy(&mut self.outbox.host_writes, &mut self.toast, &text);
        }
        if let Some(press) = link.filter(|p| p.term == id && Some(p.at) == at) {
            self.open_link(&press.target);
        }
    }

    fn press_divider(&mut self, tab: u64, path: Vec<bool>, now: Instant) {
        let double = self
            .divider_click
            .as_ref()
            .is_some_and(|(t, p, at)| *t == tab && *p == path && now.duration_since(*at) < DOUBLE_CLICK);
        if double {
            if let Some(t) = self.tab_mut().filter(|t| t.id == tab) {
                t.layout.set_ratio(&path, split::HALF);
            }
            self.divider_click = None;
        } else {
            self.divider_click = Some((tab, path.clone(), now));
            self.drag = Some(Drag::Divider { tab, path });
        }
    }

    pub(super) fn drag_divider(&mut self, tab: u64, path: &[bool], ev: MouseEvent, pane: Rect) {
        if ev.kind != MouseEventKind::Drag(MouseButton::Left) {
            self.drag = None;
            return;
        }
        self.divider_click = None;
        let Some(t) = self.tab_mut().filter(|t| t.id == tab) else { return };
        if let Some(divider) = t.layout.dividers(pane).into_iter().find(|d| d.path == path) {
            t.layout.set_ratio(path, split::ratio_at(&divider, Position::new(ev.column, ev.row)));
        }
    }

    pub(super) fn drag_pane(&mut self, drag: PaneDrag, ev: MouseEvent, pane: Rect, area: Rect) {
        let pos = Position::new(ev.column, ev.row);
        let moved = drag.moved || !drag.rect.contains(pos);
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Right) => self.drag = Some(Drag::Pane(PaneDrag { moved, ..drag })),
            MouseEventKind::Up(_) => {
                self.drag = None;
                if moved {
                    self.drop_pane(drag, pos, pane, area);
                } else {
                    self.open_pane_menu(drag.pane, drag.from, pane);
                }
            }
            _ => self.drag = None,
        }
    }

    fn drop_pane(&mut self, drag: PaneDrag, pos: Position, pane: Rect, area: Rect) {
        let Some(tab) = self.tab_mut().filter(|t| t.id == drag.tab) else { return };
        let Some(landing) = tab.landing(pane, drag.pane, pos) else { return };
        log::info!("app", "pane moved", pane = drag.pane, tab = drag.tab, place = format!("{:?}", landing.place));
        tab.layout = landing.layout;
        tab.focus(drag.pane);
        self.resize(area);
    }

    pub(super) fn pane_landing(&self, pane: Rect) -> Option<(Rect, split::Place)> {
        let drag = match &self.drag {
            Some(Drag::Pane(drag)) if drag.moved => *drag,
            Some(
                Drag::Border(_)
                | Drag::Divider { .. }
                | Drag::Row(_)
                | Drag::Pane(_)
                | Drag::Selection { .. }
                | Drag::Link(_)
                | Drag::Lines { .. },
            )
            | None => return None,
        };
        let tab = self.tab().filter(|t| t.id == drag.tab)?;
        tab.landing(pane, drag.pane, self.hover?).map(|l| (l.area, l.place))
    }

    fn open_pane_menu(&mut self, pane: u64, at: Position, area: Rect) {
        let Some(tab) = self.tab() else { return };
        if tab.rect(area, pane).is_none() {
            return;
        }
        let right_clicks =
            if tab.right_clicks_to_pane(pane) { PaneAction::RightClicksToMenu } else { PaneAction::RightClicksToPane };
        let actions = [Dir::Right, Dir::Down]
            .into_iter()
            .filter(|&d| tab.can_split(area, pane, d))
            .map(PaneAction::Split)
            .chain([right_clicks, PaneAction::Close])
            .map(|a| MenuAction::Pane(pane, a))
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn pane_action(&mut self, pane: u64, action: PaneAction, area: Rect) -> Result<()> {
        let Some((_, tab)) = self.tab_with_pane(pane) else { return Ok(()) };
        match action {
            PaneAction::Split(dir) => return self.split_pane(pane, dir, area, true).map(drop),
            PaneAction::RightClicksToPane | PaneAction::RightClicksToMenu => tab.toggle_right_clicks(pane),
            PaneAction::Close => {
                if let Some(term) = tab.panes.iter_mut().find(|t| t.id == pane) {
                    term.kill();
                }
            }
        }
        Ok(())
    }
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
