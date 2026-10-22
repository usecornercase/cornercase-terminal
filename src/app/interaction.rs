use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::input::wheel;
use super::menus::Target;
use crate::changes::selection::Spot;
use crate::files;
use crate::state;
use crate::text::stretch::Stretch;
use crate::ui::{self};

pub(super) const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub(super) const AUTO_SCROLL_EVERY: Duration = Duration::from_millis(150);

pub(super) const LINES_SCROLL_EVERY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Grab {
    Row(Target),
    Todo(u64),
    Agent(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RowDrag {
    pub(super) target: Grab,
    pub(super) row: Rect,
    pub(super) moved: bool,
    pub(super) area: Rect,
    pub(super) scrolled: Option<Instant>,
    pub(super) fold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PaneDrag {
    pub(super) tab: u64,
    pub(super) pane: u64,
    pub(super) from: Position,
    pub(super) rect: Rect,
    pub(super) moved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LinkPress {
    pub(super) term: u64,
    pub(super) at: Position,
    pub(super) target: files::link::Target,
    pub(super) held: Option<MouseEvent>,
}

#[derive(Default)]
pub(super) struct Clicks {
    border: Option<(ui::Border, Instant)>,
    divider: Option<(u64, Vec<bool>, Instant)>,
}

impl Clicks {
    pub(super) fn divider_pressed(&mut self, tab: u64, path: &[bool], now: Instant) -> bool {
        let double = self
            .divider
            .as_ref()
            .is_some_and(|(t, p, at)| *t == tab && p == path && now.duration_since(*at) < DOUBLE_CLICK);
        self.divider = (!double).then(|| (tab, path.to_vec(), now));
        double
    }

    pub(super) fn forget_divider(&mut self) {
        self.divider = None;
    }

    #[cfg(test)]
    pub(super) fn age(&mut self, by: Duration) {
        self.border = self.border.map(|(b, at)| (b, at.checked_sub(by).expect("an earlier instant")));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Anchor {
    File { line: u32, text: Option<Stretch<u32>> },
    Diff { spot: Spot, text: Option<Stretch<Spot>> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Drag {
    Border(ui::Border),
    Divider { tab: u64, path: Vec<bool> },
    Row(RowDrag),
    Pane(PaneDrag),
    Selection { term: u64, link: Option<LinkPress> },
    Link(LinkPress),
    Lines { workspace: u64, path: String, anchor: Anchor, area: Rect, moved: bool, scrolled: Option<Instant> },
    Footer(u64),
}

impl Drag {
    pub(super) fn border(&self) -> Option<ui::Border> {
        match self {
            Self::Border(border) => Some(*border),
            Self::Divider { .. }
            | Self::Row(_)
            | Self::Pane(_)
            | Self::Selection { .. }
            | Self::Link(_)
            | Self::Lines { .. }
            | Self::Footer(_) => None,
        }
    }

    pub(super) fn divider(&self) -> Option<(u64, Vec<bool>)> {
        match self {
            Self::Divider { tab, path } => Some((*tab, path.clone())),
            Self::Border(_)
            | Self::Row(_)
            | Self::Pane(_)
            | Self::Selection { .. }
            | Self::Link(_)
            | Self::Lines { .. }
            | Self::Footer(_) => None,
        }
    }

    pub(super) fn link(&self) -> Option<&LinkPress> {
        match self {
            Self::Link(press) => Some(press),
            Self::Border(_)
            | Self::Divider { .. }
            | Self::Row(_)
            | Self::Pane(_)
            | Self::Selection { .. }
            | Self::Lines { .. }
            | Self::Footer(_) => None,
        }
    }

    pub(super) fn shows_hover(&self) -> bool {
        match self {
            Self::Pane(_) => false,
            Self::Border(_)
            | Self::Divider { .. }
            | Self::Row(_)
            | Self::Selection { .. }
            | Self::Link(_)
            | Self::Lines { .. }
            | Self::Footer(_) => true,
        }
    }
}

impl App {
    pub(super) fn row_drag(&self) -> Option<RowDrag> {
        match &self.screen.drag {
            Some(Drag::Row(drag)) => Some(*drag),
            Some(
                Drag::Border(_)
                | Drag::Divider { .. }
                | Drag::Pane(_)
                | Drag::Selection { .. }
                | Drag::Link(_)
                | Drag::Lines { .. }
                | Drag::Footer(_),
            )
            | None => None,
        }
    }

    pub(super) fn press_border(&mut self, border: ui::Border, now: Instant) {
        let double =
            self.screen.clicks.border.is_some_and(|(b, at)| b == border && now.duration_since(at) < DOUBLE_CLICK);
        if double {
            self.screen.widths = self.screen.widths.reset(border);
            self.screen.clicks.border = None;
        } else {
            self.screen.clicks.border = Some((border, now));
            self.screen.drag = Some(Drag::Border(border));
        }
    }

    pub(super) fn drag_border(&mut self, border: ui::Border, ev: MouseEvent, area: Rect) {
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            let bar = self.layout(area).activity.width;
            let main = Rect { width: self.screen.widths.main_width(area.width, self.panel_shown()), ..area };
            let main = Rect { x: main.x + bar, width: main.width.saturating_sub(bar), ..main };
            let column = ev.column.saturating_sub(bar);
            self.screen.widths = match border {
                ui::Border::Changes => self.screen.widths.dragged(border, ev.column, area.width),
                ui::Border::Agents => self.agents_dragged(ev.row, area),
                _ if self.sidebar().stacked() => {
                    let pos = Position::new(column, ev.row);
                    self.screen.widths.stacked_dragged(
                        border,
                        pos,
                        main,
                        ui::Screen {
                            agents: self.config.agents_section,
                            icon_bar: self.config.icon_bar,
                            plugins: self.has_plugin_buttons(),
                            ..ui::Screen::default()
                        },
                    )
                }
                _ => self.screen.widths.dragged(border, column, main.width),
            };
            self.screen.clicks.border = None;
        } else {
            self.screen.drag = None;
        }
    }

    fn agents_dragged(&self, row: u16, area: Rect) -> state::Widths {
        let bottom = self.layout(area).agents.bottom();
        let wanted = state::Widths { agents: Some(bottom.saturating_sub(row.saturating_add(1))), ..self.screen.widths };
        let (panel, sidebar) = (self.panel_shown(), self.sidebar());
        let rows = ui::full_layout(
            area,
            wanted,
            ui::Screen {
                changes: panel,
                sidebar,
                agents: true,
                icon_bar: self.config.icon_bar,
                plugins: self.has_plugin_buttons(),
            },
        )
        .agents
        .height;
        state::Widths { agents: Some(rows), ..wanted }
    }

    pub(super) fn drag_lines(&mut self, ev: MouseEvent, panel_area: Rect) {
        let Some(Drag::Lines { workspace, path, anchor, .. }) = self.screen.drag.clone() else { return };
        if !self.shows_lines(workspace, &path, anchor) {
            self.screen.drag = None;
            return;
        }
        match ev.kind {
            MouseEventKind::Drag(MouseButton::Left) => {
                self.mark_moved();
                self.select_lines((workspace, anchor), Position::new(ev.column, ev.row), panel_area);
                self.auto_scroll_lines(Instant::now());
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(delta) = wheel(ev.kind) {
                    self.scroll_lines((workspace, anchor), panel_area, delta);
                }
                self.select_lines((workspace, anchor), Position::new(ev.column, ev.row), panel_area);
            }
            _ => self.screen.drag = None,
        }
    }

    fn mark_moved(&mut self) {
        if let Some(Drag::Lines { moved, .. }) = &mut self.screen.drag {
            *moved = true;
        }
    }

    pub(super) fn lines_edge(&self) -> Option<(Rect, Position, isize)> {
        let Some(Drag::Lines { workspace, path, anchor, area, moved: true, .. }) = &self.screen.drag else {
            return None;
        };
        let pos = self.screen.hover?;
        if !self.shows_lines(*workspace, path, *anchor) {
            return None;
        }
        let panel_area = self.layout(*area).shown(self.screen.nav).changes;
        let delta = self.lines_delta(*anchor, panel_area, pos.y)?;
        Some((panel_area, pos, delta))
    }

    pub(super) fn auto_scroll_lines(&mut self, now: Instant) {
        let Some((panel_area, pos, delta)) = self.lines_edge() else { return };
        let Some(Drag::Lines { workspace, anchor, scrolled, .. }) = &mut self.screen.drag else { return };
        if scrolled.is_some_and(|at| now.duration_since(at) < LINES_SCROLL_EVERY) {
            return;
        }
        *scrolled = Some(now);
        let at = (*workspace, *anchor);
        self.scroll_lines(at, panel_area, delta);
        self.select_lines(at, pos, panel_area);
    }

    fn shows_lines(&self, workspace: u64, path: &str, anchor: Anchor) -> bool {
        match anchor {
            Anchor::File { .. } => self.shows_file(workspace, path),
            Anchor::Diff { spot, .. } => self.shows_diff_lines(workspace, path, spot),
        }
    }

    fn lines_delta(&self, anchor: Anchor, panel_area: Rect, y: u16) -> Option<isize> {
        match anchor {
            Anchor::File { .. } => ui::files::edge(panel_area, &self.files_view()?, y),
            Anchor::Diff { .. } => ui::changes::edge(panel_area, &self.panel_view()?, y),
        }
    }

    fn scroll_lines(&mut self, (workspace, anchor): (u64, Anchor), panel_area: Rect, delta: isize) {
        match anchor {
            Anchor::File { .. } => self.scroll_viewer(workspace, panel_area, delta),
            Anchor::Diff { .. } => self.scroll_diff(panel_area, delta),
        }
    }

    fn select_lines(&mut self, (workspace, anchor): (u64, Anchor), pos: Position, panel_area: Rect) {
        match anchor {
            Anchor::File { line, text: None } => self.select_file_lines(workspace, line, pos.y, panel_area),
            Anchor::File { text: Some(text), .. } => self.select_file_text(workspace, text, pos, panel_area),
            Anchor::Diff { text: None, .. } => self.select_diff_lines(pos.y, panel_area),
            Anchor::Diff { text: Some(text), .. } => self.select_diff_text(text, pos, panel_area),
        }
    }
}
