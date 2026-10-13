use ratatui::layout::{Position, Rect};

use super::changes;
use super::layout::{Border, STACKED_KEEP, split_agents, stack_room};
use crate::state::Widths;

pub const SIDEBAR_WIDTH: u16 = 32;

pub const WORKSPACES_WIDTH: u16 = 26;

pub const PANE_PADDING: u16 = 1;

pub const MIN_COLUMN_WIDTH: u16 = 16;

pub const MIN_PANE_WIDTH: u16 = 20;

pub const MIN_STACK_SECTION: u16 = 5;

impl Default for Widths {
    fn default() -> Self {
        Self { projects: SIDEBAR_WIDTH, workspaces: WORKSPACES_WIDTH, changes: None, stack: None, agents: None }
    }
}

fn columns_room(total: u16) -> u16 {
    total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH)
}

impl Widths {
    #[must_use]
    pub fn fit(self, total: u16) -> Self {
        let room = columns_room(total);
        let workspaces = self.workspaces.min(room.saturating_sub(self.projects)).max(MIN_COLUMN_WIDTH);
        let projects = self.projects.min(room.saturating_sub(workspaces)).max(MIN_COLUMN_WIDTH);
        Self { projects, workspaces, ..self }
    }

    pub fn changes_width(self, total: u16) -> u16 {
        let max = total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH);
        let half = total.saturating_sub(self.projects + self.workspaces + PANE_PADDING) / 2;
        self.changes.unwrap_or_else(|| half.min(changes::DEFAULT_WIDTH)).max(changes::MIN_WIDTH).min(max)
    }

    pub fn main_width(self, total: u16, changes: bool) -> u16 {
        if changes { total.saturating_sub(self.changes_width(total)) } else { total }
    }

    #[must_use]
    pub fn dragged(self, border: Border, x: u16, total: u16) -> Self {
        let fitted = self.fit(total);
        let room = columns_room(total);
        let right = x.saturating_add(1);
        match border {
            Border::Projects => {
                let max = room.saturating_sub(fitted.workspaces).max(MIN_COLUMN_WIDTH);
                Self { projects: right.clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Workspaces => {
                let max = room.saturating_sub(fitted.projects).max(MIN_COLUMN_WIDTH);
                Self { workspaces: right.saturating_sub(fitted.projects).clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Changes => {
                let max = total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH);
                Self { changes: Some(total.saturating_sub(x).clamp(changes::MIN_WIDTH.min(max), max)), ..self }
            }
            Border::Stack | Border::Agents => self,
        }
    }

    #[must_use]
    pub fn reset(self, border: Border) -> Self {
        match border {
            Border::Projects => Self { projects: SIDEBAR_WIDTH, ..self },
            Border::Workspaces => Self { workspaces: WORKSPACES_WIDTH, ..self },
            Border::Stack => Self { stack: None, ..self },
            Border::Agents => Self { agents: None, ..self },
            Border::Changes => Self { changes: None, ..self },
        }
    }

    pub fn stacked_width(self, total: u16) -> u16 {
        self.projects.min(columns_room(total)).max(MIN_COLUMN_WIDTH)
    }

    pub fn top_rows(self, room: u16) -> u16 {
        if room < 2 * MIN_STACK_SECTION {
            return room / 2;
        }
        self.stack.unwrap_or(room / 2).clamp(MIN_STACK_SECTION, room - MIN_STACK_SECTION)
    }

    pub fn agents_rows(self, room: u16, keep: u16) -> u16 {
        if room < keep + MIN_STACK_SECTION {
            return room / 3;
        }
        self.agents.unwrap_or(room / 3).clamp(MIN_STACK_SECTION, room - keep)
    }

    #[must_use]
    pub fn stacked_dragged(self, border: Border, pos: Position, area: Rect, agents: bool) -> Self {
        match border {
            Border::Projects => {
                let max = columns_room(area.width).max(MIN_COLUMN_WIDTH);
                Self { projects: pos.x.saturating_add(1).clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Stack => {
                let mut room = stack_room(area);
                if agents {
                    let [rest, ..] = split_agents(Rect { height: room.height + 1, ..room }, self, STACKED_KEEP);
                    room.height = rest.height.saturating_sub(1);
                }
                let wanted = Self { stack: Some(pos.y.saturating_sub(room.y)), ..self };
                Self { stack: Some(wanted.top_rows(room.height)), ..self }
            }
            Border::Workspaces | Border::Agents | Border::Changes => self,
        }
    }
}
