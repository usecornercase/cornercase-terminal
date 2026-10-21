use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{BACK_WIDTH, FileView, Hit, PLACEHOLDER, QueryField, View, back, draw_query, full_screen_button, parts};
use crate::ui::{dim, hovered_at as hovered, put, surface_colour};

const NO_MATCHES: &str = "no matches";

pub(crate) fn find_button(area: Rect) -> Rect {
    let full = full_screen_button(area);
    if full.is_empty() {
        return Rect::default();
    }
    let button = Rect::new(full.x.saturating_sub(BACK_WIDTH), full.y, BACK_WIDTH, 1).intersection(parts(area).title);
    if button.x < back(area).right() { Rect::default() } else { button }
}

pub(super) fn find_slot(area: Rect, file: &FileView) -> Rect {
    if file.text() { find_button(area) } else { Rect::default() }
}

pub(crate) fn find_field(area: Rect) -> Rect {
    parts(area).find
}

pub(crate) fn find_close(area: Rect) -> Rect {
    let row = find_field(area);
    Rect::new(row.right().saturating_sub(BACK_WIDTH), row.y, BACK_WIDTH, 1).intersection(row)
}

pub(crate) fn find_next(area: Rect) -> Rect {
    let close = find_close(area);
    Rect::new(close.x.saturating_sub(BACK_WIDTH), close.y, BACK_WIDTH, 1).intersection(find_field(area))
}

pub(crate) fn find_previous(area: Rect) -> Rect {
    let next = find_next(area);
    Rect::new(next.x.saturating_sub(BACK_WIDTH), next.y, BACK_WIDTH, 1).intersection(find_field(area))
}

pub(super) fn find_hit(area: Rect, pos: Position) -> Option<Hit> {
    let hits = [
        (find_close(area), Hit::FindClose),
        (find_next(area), Hit::FindNext),
        (find_previous(area), Hit::FindPrevious),
        (find_field(area), Hit::FindField),
    ];
    hits.into_iter().find(|(r, _)| r.contains(pos)).map(|(_, hit)| hit)
}

fn count_text(file: &FileView) -> String {
    let Some(find) = &file.find else { return String::new() };
    if let Some((i, _)) = find.current() {
        return format!("{}/{}", i + 1, find.count());
    }
    if find.wanted().is_some() && file.content.is_some() { NO_MATCHES.to_string() } else { String::new() }
}

pub(super) fn draw_find(
    buf: &mut Buffer,
    area: Rect,
    view: &View,
    file: &FileView,
    hover: Option<Position>,
) -> Option<Position> {
    let find = file.find.as_ref()?;
    let row = find_field(area);
    if row.is_empty() {
        return None;
    }
    buf.set_style(row, Style::default().bg(surface_colour(view.light)));
    let close = find_close(area);
    let style = if hovered(hover, close) {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        dim(view.muted)
    };
    put(buf, (close.x + 1, close.y), "×", style, close.right());
    for (button, glyph) in [(find_next(area), "↓"), (find_previous(area), "↑")] {
        let style = if hovered(hover, button) { Style::default().fg(Color::Cyan) } else { dim(view.muted) };
        put(buf, (button.x + 1, button.y), glyph, style, button.right());
    }
    let count = count_text(file);
    let width = u16::try_from(count.chars().count()).unwrap_or(0);
    let end = find_previous(area).x.saturating_sub(width + u16::from(width > 0));
    put(buf, (end, row.y), &count, dim(view.muted), find_previous(area).x);
    let field = QueryField {
        glyph: view.glyphs.search,
        text: find.text(),
        placeholder: PLACEHOLDER,
        focused: find.focused,
        muted: view.muted,
    };
    draw_query(buf, row, end, &field)
}
