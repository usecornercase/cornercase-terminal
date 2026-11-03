use std::ops::Range;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use crate::text;
use crate::ui::{dim, put};

#[derive(Clone, Copy)]
pub(super) struct Look {
    pub(super) muted: Color,
    pub(super) light: bool,
    pub(super) hover: Option<Position>,
}

#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub(super) area: Rect,
    pub(super) skip: u16,
}

pub(super) fn slot(body: Rect, top: u16, scroll: u16) -> Option<Slot> {
    let y = body.y.saturating_add(top.saturating_sub(scroll));
    (y < body.bottom())
        .then(|| Slot { area: Rect { y, height: body.bottom() - y, ..body }, skip: scroll.saturating_sub(top) })
}

pub(super) fn cells(text: &str) -> u16 {
    u16::try_from(text::cells(text)).unwrap_or(u16::MAX)
}

pub(super) fn count(rows: usize) -> u16 {
    u16::try_from(rows).unwrap_or(u16::MAX)
}

impl Slot {
    pub(super) fn y(&self, row: u16) -> Option<u16> {
        let y = self.area.y.saturating_add(row.checked_sub(self.skip)?);
        (y < self.area.bottom()).then_some(y)
    }

    pub(super) fn row_at(&self, pos: Position) -> Option<u16> {
        self.area.contains(pos).then(|| (pos.y - self.area.y).saturating_add(self.skip))
    }

    pub(super) fn put(&self, buf: &mut Buffer, (row, dx): (u16, u16), text: &str, style: Style) {
        if let Some(y) = self.y(row) {
            put(buf, (self.area.x.saturating_add(dx), y), text, style, self.area.right());
        }
    }

    pub(super) fn put_right(&self, buf: &mut Buffer, row: u16, text: &str, style: Style) -> u16 {
        let width = cells(text).min(self.area.width);
        self.put(buf, (row, self.area.width - width), text, style);
        width
    }

    pub(super) fn shade(&self, buf: &mut Buffer, rows: Range<u16>, look: Look) {
        let on = look.hover.and_then(|pos| self.row_at(pos)).is_some_and(|row| rows.contains(&row));
        if !on {
            return;
        }
        let fill = Style::default().bg(Color::Indexed(if look.light { 255 } else { 235 }));
        for y in rows.filter_map(|row| self.y(row)) {
            buf.set_style(Rect { y, height: 1, ..self.area }, fill);
        }
    }

    pub(super) fn title(&self, buf: &mut Buffer, row: u16, (name, n): (&str, &str), muted: Color) {
        self.put(buf, (row, 0), name, Style::default().add_modifier(Modifier::BOLD));
        self.put(buf, (row, cells(name)), &format!(" · {n}"), dim(muted));
    }
}
