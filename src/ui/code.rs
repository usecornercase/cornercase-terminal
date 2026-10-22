use std::ops::Range;

use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;

use super::{dim, put};
use crate::highlight::syntax::Segments;
use crate::text::stretch::{shown_range, source_range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fit {
    pub(crate) shown: usize,
    pub(crate) width: u16,
    pub(crate) cut: bool,
}

fn glyphs(text: &str) -> impl Iterator<Item = (Range<usize>, u16)> {
    let mut at = 0;
    let mut start = 0;
    text.graphemes(true).filter_map(move |grapheme| {
        at += grapheme.chars().count();
        let width = if grapheme.contains(char::is_control) { 0 } else { grapheme.cell_width() };
        (width > 0).then(|| (std::mem::replace(&mut start, at)..at, width))
    })
}

pub(crate) fn fit(text: &str, room: u16) -> Fit {
    let keep = room.saturating_sub(1);
    let (mut width, mut kept) = (0u16, (0usize, 0u16));
    for (chars, cells) in glyphs(text) {
        width = width.saturating_add(cells);
        if width > room {
            return Fit { shown: kept.0, width: kept.1, cut: true };
        }
        if width <= keep {
            kept = (chars.end, width);
        }
    }
    Fit { shown: text.chars().count(), width, cut: false }
}

pub(crate) fn chars_at(text: &str, room: u16, column: u16) -> Range<usize> {
    let fit = fit(text, room);
    let mut at = 0u16;
    for (chars, cells) in glyphs(text).take_while(|(chars, _)| chars.end <= fit.shown) {
        if column < at + cells {
            return chars;
        }
        at += cells;
    }
    let total = text.chars().count();
    if fit.cut && column == fit.width { fit.shown..total } else { total..total }
}

fn columns(text: &str, room: u16, chars: &Range<usize>) -> Option<Range<u16>> {
    let fit = fit(text, room);
    let mut at = 0u16;
    let mut found: Option<Range<u16>> = None;
    for (glyph, cells) in glyphs(text).take_while(|(glyph, _)| glyph.end <= fit.shown) {
        if glyph.start < chars.end && chars.start < glyph.end {
            found = Some(found.map_or(at..at + cells, |f| f.start..at + cells));
        }
        at += cells;
    }
    if fit.cut && !chars.is_empty() && chars.end > fit.shown {
        found = Some(found.map_or(fit.width..fit.width + 1, |f| f.start..fit.width + 1));
    }
    found
}

pub(crate) fn source_at(code: Rect, text: &str, raw: &str, x: u16) -> Range<usize> {
    if x < code.x {
        return 0..0;
    }
    source_range(raw, chars_at(text, code.width.saturating_sub(1), x - code.x))
}

pub(crate) fn paint(buf: &mut Buffer, code: Rect, (text, raw): (&str, &str), on: &(Range<usize>, bool)) {
    let (chars, broken) = on;
    let reversed = Style::new().add_modifier(Modifier::REVERSED);
    if *broken && chars.start == 0 {
        buf.set_style(code, reversed);
        return;
    }
    let room = code.width.saturating_sub(1);
    let found = columns(text, room, &shown_range(raw, chars.clone()));
    let edge = fit(text, room);
    let start = found.as_ref().map_or(edge.width + u16::from(edge.cut), |f| f.start);
    let end = if *broken { code.width } else { found.map_or(start, |f| f.end) };
    buf.set_style(Rect::new(code.x + start, code.y, end.saturating_sub(start), 1).intersection(code), reversed);
}

pub(crate) fn clip(segments: &[(String, Style)], keep: usize) -> Segments {
    let mut left = keep;
    let mut out = Segments::new();
    for (text, style) in segments {
        if left == 0 {
            break;
        }
        let piece: String = text.chars().take(left).collect();
        left -= piece.chars().count();
        out.push((piece, *style));
    }
    out
}

pub(crate) fn draw(buf: &mut Buffer, muted: Color, r: Rect, segments: &[(String, Style)]) {
    let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
    draw_fitted(buf, muted, r, segments, fit(&text, r.width.saturating_sub(1)));
}

pub(crate) fn draw_fitted(buf: &mut Buffer, muted: Color, r: Rect, segments: &[(String, Style)], fit: Fit) {
    let end = r.right().saturating_sub(1);
    let mut x = r.x;
    for (piece, style) in clip(segments, fit.shown) {
        let style = if style.fg == Some(Color::DarkGray) { style.fg(muted) } else { style };
        x = put(buf, (x, r.y), &piece, style, end);
    }
    if fit.cut {
        put(buf, (r.x + fit.width, r.y), "…", dim(muted), end + 1);
    }
}

#[cfg(test)]
mod tests;
