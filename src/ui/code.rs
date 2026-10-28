use std::ops::Range;

use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;

use super::{dim, put};
use crate::highlight::syntax::Segments;
use crate::text::stretch::{shown_range, source_range};

pub(crate) const MAX_WRAPPED_ROWS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Line,
    Wrap,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fit {
    pub(crate) chars: Range<usize>,
    pub(crate) width: u16,
    pub(crate) end: End,
}

type Glyph = (Range<usize>, u16);

fn glyphs(text: &str) -> impl Iterator<Item = Glyph> {
    let mut at = 0;
    let mut start = 0;
    text.graphemes(true).filter_map(move |grapheme| {
        at += grapheme.chars().count();
        let width = if grapheme.contains(char::is_control) { 0 } else { grapheme.cell_width() };
        (width > 0).then(|| (std::mem::replace(&mut start, at)..at, width))
    })
}

fn tail(text: &str, chars: usize) -> &str {
    text.char_indices().nth(chars).map_or("", |(byte, _)| &text[byte..])
}

fn take(rest: &str, room: u16) -> (Vec<Glyph>, bool) {
    let (mut width, mut taken) = (0u16, Vec::new());
    for (chars, cells) in glyphs(rest) {
        width = width.saturating_add(cells);
        if width > room {
            return (taken, true);
        }
        taken.push((chars, cells));
    }
    (taken, false)
}

fn breakable(c: char) -> bool {
    c.is_whitespace() || c.is_ascii_punctuation()
}

fn break_at(rest: &str, taken: &[Glyph]) -> usize {
    let chars: Vec<char> = rest.chars().take(taken.last().map_or(0, |(g, _)| g.end)).collect();
    taken.iter().rposition(|(g, _)| g.len() == 1 && breakable(chars[g.start])).map_or(taken.len(), |i| i + 1)
}

fn keep_within(taken: &[Glyph], cells: u16) -> usize {
    let mut width = 0u16;
    taken
        .iter()
        .take_while(|(_, w)| {
            width = width.saturating_add(*w);
            width <= cells
        })
        .count()
}

fn next_row(rest: &str, base: usize, room: u16, last: bool) -> Fit {
    let (taken, more) = take(rest, room);
    let (count, end) = match (more, last || taken.is_empty()) {
        (false, _) => (taken.len(), End::Line),
        (true, true) => (keep_within(&taken, room.saturating_sub(1)), End::Cut),
        (true, false) => (break_at(rest, &taken), End::Wrap),
    };
    let taken = &taken[..count];
    let width = taken.iter().map(|(_, w)| *w).sum();
    let stop = if end == End::Line { rest.chars().count() } else { taken.last().map_or(0, |(g, _)| g.end) };
    Fit { chars: base..base + stop, width, end }
}

fn lay_out(text: &str, room: u16, most: usize) -> Vec<Fit> {
    let mut rows: Vec<Fit> = Vec::new();
    let (mut rest, mut base) = (text, 0);
    while rows.len() < most {
        let row = next_row(rest, base, room, rows.len() + 1 == most);
        let (end, stop) = (row.end, row.chars.end);
        rows.push(row);
        if end != End::Wrap {
            break;
        }
        rest = tail(rest, stop - base);
        base = stop;
    }
    rows
}

pub(crate) fn wrap(text: &str, room: u16) -> Vec<Fit> {
    lay_out(text, room, MAX_WRAPPED_ROWS)
}

pub(crate) fn rows(text: &str, room: u16) -> usize {
    if text.len() <= usize::from(room) { 1 } else { wrap(text, room).len() }
}

pub(crate) fn fit(text: &str, room: u16) -> Fit {
    next_row(text, 0, room, true)
}

fn inside<'a>(text: &'a str, row: &'a Fit) -> impl Iterator<Item = Glyph> + 'a {
    let start = row.chars.start;
    glyphs(tail(text, start))
        .map(move |(g, w)| (g.start + start..g.end + start, w))
        .take_while(|(g, _)| g.end <= row.chars.end)
}

pub(crate) fn chars_at(text: &str, row: &Fit, column: u16) -> Range<usize> {
    let mut at = 0u16;
    for (chars, cells) in inside(text, row) {
        if column < at + cells {
            return chars;
        }
        at += cells;
    }
    match row.end {
        End::Wrap => row.chars.end..row.chars.end,
        End::Cut if column == row.width => row.chars.end..text.chars().count(),
        End::Line | End::Cut => text.chars().count()..text.chars().count(),
    }
}

fn columns(text: &str, row: &Fit, chars: &Range<usize>) -> Option<Range<u16>> {
    let mut at = 0u16;
    let mut found: Option<Range<u16>> = None;
    for (glyph, cells) in inside(text, row) {
        if glyph.start < chars.end && chars.start < glyph.end {
            found = Some(found.map_or(at..at + cells, |f| f.start..at + cells));
        }
        at += cells;
    }
    if row.end == End::Cut && !chars.is_empty() && chars.end > row.chars.end {
        found = Some(found.map_or(row.width..row.width + 1, |f| f.start..row.width + 1));
    }
    found
}

pub(crate) fn source_at(code: Rect, (text, raw): (&str, &str), row: &Fit, x: u16) -> Range<usize> {
    if x < code.x {
        return source_range(raw, row.chars.start..row.chars.start);
    }
    source_range(raw, chars_at(text, row, x - code.x))
}

fn holds(row: &Fit, sel: &Range<usize>, broken: bool, total: usize) -> bool {
    let limit = if row.end == End::Cut { total } else { row.chars.end };
    let text = sel.start < limit && sel.end > row.chars.start;
    text || (row.end != End::Wrap && broken && sel.start <= row.chars.end)
}

pub(crate) fn paint(buf: &mut Buffer, code: Rect, (text, raw): (&str, &str), row: &Fit, on: &(Range<usize>, bool)) {
    let (chars, broken) = on;
    let sel = shown_range(raw, chars.clone());
    if !holds(row, &sel, *broken, text.chars().count()) {
        return;
    }
    let found = columns(text, row, &sel);
    let past = *broken || (row.end == End::Wrap && sel.end > row.chars.end);
    let edge = row.width + u16::from(row.end == End::Cut);
    let start = if sel.start <= row.chars.start { 0 } else { found.as_ref().map_or(edge, |f| f.start) };
    let end = if past { code.width } else { found.map_or(start, |f| f.end) };
    let reversed = Style::new().add_modifier(Modifier::REVERSED);
    buf.set_style(Rect::new(code.x + start, code.y, end.saturating_sub(start), 1).intersection(code), reversed);
}

pub(crate) fn slice(segments: &[(String, Style)], chars: &Range<usize>) -> Segments {
    let (mut skip, mut left) = (chars.start, chars.len());
    let mut out = Segments::new();
    for (text, style) in segments {
        if left == 0 {
            break;
        }
        let piece: String = text.chars().skip(skip).take(left).collect();
        skip = skip.saturating_sub(text.chars().count());
        left -= piece.chars().count();
        if !piece.is_empty() {
            out.push((piece, *style));
        }
    }
    out
}

pub(crate) fn slice_text(text: &str, chars: &Range<usize>) -> String {
    text.chars().skip(chars.start).take(chars.len()).collect()
}

pub(crate) fn draw(buf: &mut Buffer, muted: Color, r: Rect, segments: &[(String, Style)]) {
    let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
    let fit = fit(&text, r.width.saturating_sub(1));
    draw_row(buf, muted, r, &slice(segments, &fit.chars), &fit);
}

pub(crate) fn draw_row(buf: &mut Buffer, muted: Color, r: Rect, segments: &[(String, Style)], fit: &Fit) {
    let end = r.right().saturating_sub(1);
    let mut x = r.x;
    for (piece, style) in segments {
        let style = if style.fg == Some(Color::DarkGray) { style.fg(muted) } else { *style };
        x = put(buf, (x, r.y), piece, style, end);
    }
    if fit.end == End::Cut {
        put(buf, (r.x + fit.width, r.y), "…", dim(muted), end + 1);
    }
}

#[cfg(test)]
mod tests;
