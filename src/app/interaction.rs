use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::Target;
use crate::files;
use crate::ui::{self};

pub(super) const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub(super) const AUTO_SCROLL_EVERY: Duration = Duration::from_millis(150);

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

impl App {
    pub(super) fn press_border(&mut self, border: ui::Border, now: Instant) {
        let double = self.border_click.is_some_and(|(b, at)| b == border && now.duration_since(at) < DOUBLE_CLICK);
        if double {
            self.widths = self.widths.reset(border);
            self.border_click = None;
        } else {
            self.border_click = Some((border, now));
            self.resizing = Some(border);
        }
    }

    pub(super) fn drag_border(&mut self, border: ui::Border, ev: MouseEvent, area: Rect) {
        if ev.kind == MouseEventKind::Drag(MouseButton::Left) {
            let main = Rect { width: self.widths.main_width(area.width, self.panel_shown()), ..area };
            self.widths = match border {
                ui::Border::Changes => self.widths.dragged(border, ev.column, area.width),
                ui::Border::Agents => self.agents_dragged(ev.row, area),
                _ if self.sidebar().stacked() => {
                    let pos = Position::new(ev.column, ev.row);
                    self.widths.stacked_dragged(border, pos, main, self.config.agents_section)
                }
                _ => self.widths.dragged(border, ev.column, main.width),
            };
            self.border_click = None;
        } else {
            self.resizing = None;
        }
    }

    fn agents_dragged(&self, row: u16, area: Rect) -> ui::Widths {
        let bottom = self.layout(area).agents.bottom();
        let wanted = ui::Widths { agents: Some(bottom.saturating_sub(row.saturating_add(1))), ..self.widths };
        let (panel, sidebar) = (self.panel_shown(), self.sidebar());
        let rows = ui::full_layout(area, wanted, panel, sidebar, true).agents.height;
        ui::Widths { agents: Some(rows), ..wanted }
    }
}
