use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use super::stack::{Look, Slot, cells, count};
use super::{Hit, dim};
use crate::ui::widgets::action_style;

const TITLE: &str = "Files";
const EMPTY: &str = "nothing yet";
const FOLDER: &str = " open folder ";
const LEAD: u16 = 2;
const MIN_NAME: u16 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) name: String,
    pub(crate) size: String,
    pub(crate) age: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Files {
    pub(crate) rows: Vec<FileRow>,
    pub(crate) more: bool,
}

pub(super) fn height(files: Option<&Files>) -> u16 {
    files.map_or(0, |files| LEAD.saturating_add(count(files.rows.len().max(1))))
}

fn folder(slot: &Slot) -> Option<Rect> {
    let y = slot.y(1)?;
    let width = cells(FOLDER).min(slot.area.width);
    Some(Rect { x: slot.area.right() - width, y, width, height: 1 })
}

pub(super) fn hit(slot: &Slot, files: &Files, pos: Position) -> Option<Hit> {
    if folder(slot).is_some_and(|rect| rect.contains(pos)) {
        return Some(Hit::Folder);
    }
    let row = slot.row_at(pos)?.checked_sub(LEAD)?;
    let index = usize::from(row);
    (index < files.rows.len()).then_some(Hit::File(index))
}

fn draw_row(buf: &mut Buffer, slot: &Slot, (i, file): (u16, &FileRow), muted: ratatui::style::Color) {
    let row = LEAD + i;
    let width = slot.area.width;
    let both = format!("{} · {}", file.size, file.age);
    let tail = if width.saturating_sub(cells(&both).saturating_add(1)) >= MIN_NAME { both } else { file.age.clone() };
    let tail = slot.put_right(buf, row, &tail, dim(muted));
    let room = usize::from(width.saturating_sub(tail.saturating_add(1)));
    slot.put(buf, (row, 0), &crate::text::truncate_cells(&file.name, room), Style::default());
}

pub(super) fn draw(buf: &mut Buffer, slot: &Slot, files: &Files, look: Look) {
    let n = if files.more { format!("{}+", files.rows.len()) } else { files.rows.len().to_string() };
    slot.title(buf, 1, (TITLE, &n), look.muted);
    if let Some(rect) = folder(slot) {
        let lit = look.hover.is_some_and(|pos| rect.contains(pos));
        slot.put(buf, (1, rect.x - slot.area.x), FOLDER, action_style(lit));
    }
    if files.rows.is_empty() {
        slot.put(buf, (LEAD, 0), EMPTY, dim(look.muted));
        return;
    }
    for (i, file) in files.rows.iter().enumerate() {
        let i = count(i);
        slot.shade(buf, LEAD + i..LEAD + i + 1, look);
        draw_row(buf, slot, (i, file), look.muted);
    }
}
