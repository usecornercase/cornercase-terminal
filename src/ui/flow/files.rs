use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};

use super::{Color, Hit, dim, put};
use crate::text;

const TITLE: &str = "Files";
const EMPTY: &str = "nothing yet";
const LEAD: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRow {
    pub(crate) name: String,
    pub(crate) age: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Files {
    pub(crate) rows: Vec<FileRow>,
    pub(crate) more: bool,
}

pub(super) fn height(files: Option<&Files>) -> u16 {
    files.map_or(0, |files| LEAD.saturating_add(u16::try_from(files.rows.len().max(1)).unwrap_or(u16::MAX)))
}

fn content_row(area: Rect, scroll: u16, y: u16) -> Option<u16> {
    (y >= area.y && y < area.bottom()).then(|| (y - area.y).saturating_add(scroll))
}

pub(super) fn hit(area: Rect, files: &Files, top: u16, scroll: u16, pos: Position) -> Option<Hit> {
    if !area.contains(pos) {
        return None;
    }
    let row = content_row(area, scroll, pos.y)?.checked_sub(top.saturating_add(LEAD))?;
    let index = usize::from(row);
    (index < files.rows.len()).then_some(Hit::File(index))
}

pub(super) fn draw(
    buf: &mut Buffer,
    area: Rect,
    files: &Files,
    (top, scroll): (u16, u16),
    muted: Color,
    light: bool,
    hover: Option<Position>,
) {
    let at = |row: u16| {
        let content = top.saturating_add(row);
        let y = content.checked_sub(scroll)?;
        (y < area.height).then(|| area.y + y)
    };
    if let Some(y) = at(1) {
        let count = if files.more { format!("{}+", files.rows.len()) } else { files.rows.len().to_string() };
        put(buf, (area.x, y), TITLE, Style::default().add_modifier(Modifier::BOLD), area.right());
        let after = area.x + u16::try_from(text::cells(TITLE)).unwrap_or(0);
        put(buf, (after, y), &format!(" · {count}"), dim(muted), area.right());
    }
    if files.rows.is_empty() {
        if let Some(y) = at(LEAD) {
            put(buf, (area.x, y), EMPTY, dim(muted), area.right());
        }
        return;
    }
    let fill = Style::default().bg(Color::Indexed(if light { 255 } else { 235 }));
    for (i, row) in files.rows.iter().enumerate() {
        let Some(y) = u16::try_from(i).ok().and_then(|i| at(LEAD + i)) else { continue };
        if hover.is_some_and(|p| p.y == y && area.contains(p)) {
            buf.set_style(Rect { y, height: 1, ..area }, fill);
        }
        let age = u16::try_from(text::cells(&row.age)).unwrap_or(0).min(area.width);
        let room = usize::from(area.width.saturating_sub(age.saturating_add(1)));
        put(buf, (area.x, y), &text::truncate_cells(&row.name, room), Style::default(), area.right());
        put(buf, (area.right() - age, y), &row.age, dim(muted), area.right());
    }
}
