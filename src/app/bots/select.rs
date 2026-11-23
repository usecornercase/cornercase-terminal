use std::ops::Range;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::input::wheel;
use crate::app::interaction::Drag;
use crate::ui::{self, ChatAreas, ChatView};

const HALF_BLOCKS: [char; 2] = ['▄', '▀'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct Cell {
    pub(in crate::app) up: usize,
    pub(in crate::app) col: u16,
}

impl Cell {
    fn before(self, other: Self) -> bool {
        self.up > other.up || (self.up == other.up && self.col <= other.col)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct Selection {
    pub(in crate::app) anchor: Cell,
    pub(in crate::app) head: Cell,
}

impl Selection {
    fn at(cell: Cell) -> Self {
        Self { anchor: cell, head: cell }
    }

    fn ends(self) -> (Cell, Cell) {
        if self.anchor.before(self.head) { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }

    pub(in crate::app) fn cols(self, up: usize) -> Option<Range<u16>> {
        let (start, end) = self.ends();
        if up > start.up || up < end.up {
            return None;
        }
        let from = if up == start.up { start.col } else { 0 };
        let to = if up == end.up { end.col.saturating_add(1) } else { u16::MAX };
        Some(from..to)
    }

    pub(in crate::app) fn moved(self, up: impl Fn(usize) -> Option<usize>) -> Option<Self> {
        let lift = |cell: Cell| up(cell.up).map(|up| Cell { up, ..cell });
        Some(Self { anchor: lift(self.anchor)?, head: lift(self.head)? })
    }
}

fn decoration(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty() && text.chars().all(|c| HALF_BLOCKS.contains(&c))
}

fn joined(rows: impl Iterator<Item = String>) -> String {
    let kept: Vec<String> = rows.map(|row| row.trim_end().to_string()).filter(|row| !decoration(row)).collect();
    let indent = |row: &String| row.len() - row.trim_start_matches(' ').len();
    let common = kept.iter().filter(|row| !row.is_empty()).map(indent).min().unwrap_or(0);
    let lines: Vec<&str> = kept.iter().map(|row| row.get(common..).unwrap_or_default()).collect();
    lines.join("\n").trim_matches('\n').to_string()
}

impl App {
    pub(super) fn clear_chat_selection(&mut self) {
        if let Some(shown) = self.panels.bots.shown.as_mut() {
            shown.selection = None;
        }
    }

    pub(super) fn press_chat_text(&mut self, areas: &ChatAreas, view: &ChatView, pos: Position) {
        let body = ui::chat_body(areas.lines, view.thinking.is_some());
        let cell = ui::chat_cell(areas, view, pos).filter(|_| body.contains(pos));
        let Some(shown) = self.panels.bots.shown.as_mut() else { return };
        shown.selection = cell.map(|(row, col)| Selection::at(Cell { up: up_of(shown.scroll, view, row), col }));
        if shown.selection.is_some() {
            self.screen.drag = Some(Drag::Chat);
        }
    }

    pub(in crate::app) fn drag_chat(&mut self, ev: MouseEvent, area: Rect) {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_chat(delta < 0, delta.unsigned_abs());
            return;
        }
        if ev.kind != MouseEventKind::Drag(MouseButton::Left) {
            self.screen.drag = None;
            self.end_chat_selection(area);
            return;
        }
        let pos = Position::new(ev.column, ev.row);
        let thinking = self.shown_bot().is_some_and(|at| self.panels.bots.live[at].turn.is_some());
        let body = ui::chat_body(self.chat_geometry(area).lines, thinking);
        if pos.y < body.y {
            self.scroll_chat(true, 1);
        } else if pos.y >= body.bottom() {
            self.scroll_chat(false, 1);
        }
        let head = self.chat_cell_at(pos, area);
        let selection = self.panels.bots.shown.as_mut().and_then(|shown| shown.selection.as_mut());
        if let (Some(selection), Some(head)) = (selection, head) {
            selection.head = head;
        }
    }

    fn chat_cell_at(&mut self, pos: Position, area: Rect) -> Option<Cell> {
        let shown = self.shown_chat(area)?;
        let areas = self.chat_geometry(area);
        let (row, col) = ui::chat_cell(&areas, &shown.view, pos)?;
        let scroll = self.panels.bots.shown.as_ref()?.scroll;
        Some(Cell { up: up_of(scroll, &shown.view, row), col })
    }

    fn end_chat_selection(&mut self, area: Rect) {
        let selection = self.panels.bots.shown.as_ref().and_then(|shown| shown.selection);
        let text = selection
            .filter(|selection| selection.anchor != selection.head)
            .and_then(|selection| self.chat_selected_text(selection, area).filter(|text| !text.trim().is_empty()));
        match text {
            Some(text) => self.copy(&text),
            None => self.clear_chat_selection(),
        }
    }

    fn chat_selected_text(&mut self, selection: Selection, area: Rect) -> Option<String> {
        let at = self.shown_bot()?;
        let (facts, tone) = (self.chat_facts(at), self.chat_tone());
        let width = self.chat_geometry(area).lines.width;
        let (start, end) = selection.ends();
        let shown = self.panels.bots.shown.as_mut()?;
        let (rows, _) = shown.rows(&facts, width, start.up + 1, tone);
        let picked = (end.up..=start.up).rev().filter_map(|up| {
            let row = rows.get(rows.len().checked_sub(up + 1)?)?;
            Some(ui::row_cells(row, &selection.cols(up)?))
        });
        Some(joined(picked))
    }
}

fn up_of(scroll: usize, view: &ChatView, row: usize) -> usize {
    scroll + view.rows.len().saturating_sub(row + 1)
}

#[cfg(test)]
mod tests;
