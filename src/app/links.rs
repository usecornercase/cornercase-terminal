use std::ops::Range;
use std::path::Path;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::LinkPress;
use crate::emulator::Snapshot;
use crate::files::link::{self, Target};
use crate::mouse::MouseMode;
use crate::project::Tab;

impl App {
    fn link_at(&mut self, at: Position) -> Option<Target> {
        let root = self.project()?.workspace()?.path.clone();
        let home = self.home.clone();
        let term = self.term_mut()?;
        let screen = term.emulator.snapshot().ok()?;
        let found = link::at(screen.rows.get(usize::from(at.y))?, at.x)?;
        link::resolve(&found, term.cwd().as_deref(), &root, home.as_deref())
    }

    pub(super) fn link_click(&mut self, ev: MouseEvent, at: Position) -> bool {
        let Some((id, reads)) = self.term().map(|t| (t.id, t.emulator.mouse_mode() != MouseMode::None)) else {
            return false;
        };
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.link_press = None;
                let Some(target) = self.link_at(at) else { return false };
                self.link_press = Some(LinkPress { term: id, at, target, held: reads.then_some(ev) });
                reads
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(press) = self.link_press.clone().filter(|p| p.term == id && p.held.is_some()) else {
                    return false;
                };
                if press.at == at {
                    return true;
                }
                self.link_press = None;
                if let Some(held) = press.held {
                    self.forward_mouse(held, press.at);
                }
                false
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(press) = self.link_press.take().filter(|p| p.term == id && p.held.is_some()) else {
                    return false;
                };
                if press.at == at {
                    self.open_link(&press.target);
                    return true;
                }
                if let Some(held) = press.held {
                    self.forward_mouse(held, press.at);
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn open_link(&mut self, target: &Target) {
        let Some(workspace) = self.project().and_then(crate::project::Project::workspace).map(|w| w.id) else {
            return;
        };
        if !self.files.open {
            self.changes.close();
            self.close_todo();
            self.files.open = true;
        }
        self.nav = None;
        match target.lines {
            Some(lines) => self.files.open_at(workspace, &target.path, lines, None),
            None => self.files.open_file(workspace, &target.path),
        }
    }

    pub(super) fn link_hover(&self) -> Option<Position> {
        self.hover.filter(|_| {
            self.overlay.is_none()
                && self.row_drag.is_none()
                && self.pane_drag.is_none()
                && self.selecting.is_none()
                && self.divider_drag.is_none()
        })
    }

    pub(super) fn hovered_link(
        tab: &Tab,
        screens: &[Snapshot],
        pane_area: Rect,
        hover: Position,
        root: &Path,
        home: Option<&Path>,
    ) -> Option<(u16, Range<u16>)> {
        let term = tab.pane()?;
        let pane = tab.rect(pane_area, term.id)?;
        if !pane.contains(hover) {
            return None;
        }
        let (row, col) = (hover.y - pane.y, hover.x - pane.x);
        let found = link::at(screens.get(tab.active)?.rows.get(usize::from(row))?, col)?;
        link::resolve(&found, term.cwd().as_deref(), root, home)?;
        Some((row, found.start..found.end))
    }
}
