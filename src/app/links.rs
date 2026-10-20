use std::ops::Range;
use std::path::Path;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::{Drag, LinkPress};
use super::layout::Side;
use crate::files::link::{self, Target};
use crate::project::Tab;
use crate::term::emulator::Snapshot;
use crate::term::mouse::MouseMode;

#[derive(Clone, Copy)]
pub(super) struct Resolving<'a> {
    pub(super) root: &'a Path,
    pub(super) home: Option<&'a Path>,
}

impl App {
    fn link_at(&mut self, at: Position) -> Option<Target> {
        let root = self.project()?.workspace()?.path.clone();
        let home = self.home.clone();
        let term = self.term_mut()?;
        let screen = term.emulator.snapshot().ok()?;
        link::under(screen.rows.get(usize::from(at.y))?, at.x, term.cwd().as_deref(), &root, home.as_deref())
            .map(|(_, target)| target)
    }

    pub(super) fn link_click(&mut self, ev: MouseEvent, at: Position) -> bool {
        let Some((id, reads)) = self.term().map(|t| (t.id, t.emulator.mouse_mode() != MouseMode::None)) else {
            return false;
        };
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.drag = None;
                let Some(target) = self.link_at(at) else { return false };
                self.drag = Some(Drag::Link(LinkPress { term: id, at, target, held: reads.then_some(ev) }));
                reads
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(press) = self.held_link().filter(|p| p.term == id) else { return false };
                let press = press.clone();
                if press.at == at {
                    return true;
                }
                self.drag = None;
                if let Some(held) = press.held {
                    self.forward_mouse(held, press.at);
                }
                false
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let press = self.held_link().filter(|p| p.term == id).cloned();
                self.drag = None;
                let Some(press) = press else { return false };
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

    fn held_link(&self) -> Option<&LinkPress> {
        self.drag.as_ref().and_then(Drag::link).filter(|press| press.held.is_some())
    }

    pub(super) fn open_link(&mut self, target: &Target) {
        let Some(workspace) = self.project().and_then(crate::project::Project::workspace).map(|w| w.id) else {
            return;
        };
        self.show_side(Some(Side::Files));
        self.nav = None;
        match target.lines {
            Some(lines) => self.files.open_at(workspace, &target.path, lines, None),
            None => self.files.open_file(workspace, &target.path),
        }
    }

    pub(super) fn link_hover(&self) -> Option<Position> {
        self.hover.filter(|_| {
            self.overlay.is_none()
                && !self.file_full_screen()
                && match &self.drag {
                    Some(Drag::Row(_) | Drag::Pane(_) | Drag::Selection { .. } | Drag::Divider { .. }) => false,
                    Some(Drag::Border(_) | Drag::Link(_) | Drag::Lines { .. }) | None => true,
                }
        })
    }

    pub(super) fn hovered_link(
        tab: &Tab,
        screens: &[Snapshot],
        pane_area: Rect,
        hover: Position,
        resolving: Resolving<'_>,
    ) -> Option<(u16, Range<u16>)> {
        let term = tab.pane()?;
        let pane = tab.rect(pane_area, term.id)?;
        if !pane.contains(hover) {
            return None;
        }
        let (row, col) = (hover.y - pane.y, hover.x - pane.x);
        let line = screens.get(tab.active)?.rows.get(usize::from(row))?;
        let (found, _) = link::under(line, col, term.cwd().as_deref(), resolving.root, resolving.home)?;
        Some((row, found.start..found.end))
    }
}
