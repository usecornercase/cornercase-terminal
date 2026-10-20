use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::Target;
use crate::files;
use crate::state;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Drag {
    Border(ui::Border),
    Divider { tab: u64, path: Vec<bool> },
    Row(RowDrag),
    Pane(PaneDrag),
    Selection { term: u64, link: Option<LinkPress> },
    Link(LinkPress),
    Lines { workspace: u64, path: String, anchor: u32, area: Rect, moved: bool, scrolled: Option<Instant> },
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
                            icons: self.config.icon_bar,
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
            ui::Screen { changes: panel, sidebar, agents: true, icons: self.config.icon_bar },
        )
        .agents
        .height;
        state::Widths { agents: Some(rows), ..wanted }
    }
}
