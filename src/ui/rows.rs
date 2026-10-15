use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::widgets::{centered, draw_band, hovered, sidebar_hovered};

pub(super) const GAP: u16 = 1;

pub(super) const CLOSE_BUTTON_WIDTH: u16 = 3;

pub(super) const COMPACT_CLOSE_WIDTH: u16 = 5;

pub(super) const MENU_BUTTON_WIDTH: u16 = 2;

const COMPACT_MENU_WIDTH: u16 = 4;

pub(super) const MIN_MENU_ROW_WIDTH: u16 = 20;

pub(crate) const ROW_MENU_ICON: &str = "⋯";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    pub(super) list: Rect,
    pub(super) heights: Vec<u16>,
    pub(super) button: u16,
    pub(super) scroll: usize,
}

impl Rows {
    fn room(&self) -> u16 {
        self.list.height.saturating_sub(GAP + self.button)
    }

    fn height(&self, range: Range<usize>) -> u32 {
        self.heights[range].iter().map(|&h| u32::from(h)).sum()
    }

    fn fitting_before(&self, end: usize) -> usize {
        let room = u32::from(self.room());
        let mut start = end;
        while start > 0 && self.height(start - 1..end) <= room {
            start -= 1;
        }
        start
    }

    pub(super) fn first(&self) -> usize {
        self.scroll.min(self.fitting_before(self.heights.len()))
    }

    pub(super) fn end(&self) -> usize {
        let (first, room) = (self.first(), u32::from(self.room()));
        (first..self.heights.len()).take_while(|&i| self.height(first..i + 1) <= room).last().map_or(first, |i| i + 1)
    }

    pub(crate) fn item(&self, i: usize) -> Rect {
        let first = self.first();
        if !(first..self.end()).contains(&i) {
            return Rect::default();
        }
        let y = u32::from(self.list.y) + self.height(first..i);
        Rect::new(self.list.x, u16::try_from(y).unwrap_or(u16::MAX), self.list.width, self.heights[i])
    }

    pub(crate) fn at(&self, pos: Position) -> Option<usize> {
        (self.first()..self.end()).find(|&i| self.item(i).contains(pos))
    }

    pub(crate) fn hidden(&self) -> (Range<usize>, Range<usize>) {
        (0..self.first(), self.end()..self.heights.len())
    }

    pub(crate) fn scrolled(&self, delta: isize) -> usize {
        let max = self.fitting_before(self.heights.len());
        self.scroll.min(max).saturating_add_signed(delta).min(max)
    }

    pub(crate) fn reveal(&self, i: usize) -> usize {
        let first = self.first();
        if i >= self.heights.len() || (first..self.end()).contains(&i) {
            first
        } else if i < first {
            i
        } else {
            self.fitting_before(i + 1).min(i)
        }
    }

    pub fn button(&self) -> Rect {
        let gap = if self.heights.is_empty() { 0 } else { u32::from(GAP) };
        let below = u32::from(self.list.y) + self.height(0..self.heights.len()) + gap;
        let last = self.list.bottom().saturating_sub(self.button);
        let y = u16::try_from(below).unwrap_or(u16::MAX).min(last);
        Rect::new(self.list.x, y, self.list.width, self.button)
    }

    pub(crate) fn more_below(&self) -> Rect {
        Rect::new(self.list.x, self.list.y.saturating_add(self.room()), self.list.width, 1).intersection(self.list)
    }

    pub(crate) fn edge(&self, pos: Position) -> Option<isize> {
        let (above, below) = self.hidden();
        let y = self.list.y.saturating_add(self.room());
        let bottom = Rect { y, height: self.list.bottom().saturating_sub(y), ..self.list };
        if !above.is_empty() && more_above(self.list).contains(pos) {
            Some(-1)
        } else if !below.is_empty() && bottom.contains(pos) {
            Some(1)
        } else {
            None
        }
    }

    pub(super) fn zone(&self, pos: Position) -> Option<Zone> {
        if more_above(self.list).contains(pos) {
            return Some(Zone::Above);
        }
        if !self.list.contains(pos) {
            return None;
        }
        Some(self.at(pos).map_or(Zone::Below, Zone::Row))
    }

    pub(super) fn boundary(&self, zone: Zone, on_row: impl Fn(usize) -> Option<usize>) -> Option<usize> {
        match zone {
            Zone::Above => Some(self.first()),
            Zone::Row(i) => on_row(i),
            Zone::Below => Some(self.end()),
        }
    }
}

pub(crate) fn more_above(list: Rect) -> Rect {
    if list.y < GAP { Rect::default() } else { Rect::new(list.x, list.y - GAP, list.width, 1) }
}

pub(super) fn row_rect<R: PartialEq>(layout: &Rows, rows: &[R], row: &R) -> Rect {
    rows.iter().position(|r| r == row).map_or_else(Rect::default, |i| layout.item(i))
}

pub fn row_close_button(row: Rect, pitch: u16) -> Rect {
    let width = if pitch > 1 { COMPACT_CLOSE_WIDTH } else { CLOSE_BUTTON_WIDTH };
    Rect::new(row.right().saturating_sub(width), row.y, width.min(row.width), pitch.min(row.height))
}

pub(crate) fn row_menu_button(row: Rect, pitch: u16) -> Rect {
    let close = row_close_button(row, pitch);
    let width = match pitch {
        _ if row.width < MIN_MENU_ROW_WIDTH => 0,
        1 => MENU_BUTTON_WIDTH,
        _ => COMPACT_MENU_WIDTH,
    };
    let x = close.x.saturating_sub(width).max(row.x);
    Rect::new(x, row.y, close.x - x, close.height)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RowButton {
    Close,
    Menu,
}

pub(super) fn row_button(row: Rect, pitch: u16, pos: Position) -> Option<RowButton> {
    if row_close_button(row, pitch).contains(pos) {
        Some(RowButton::Close)
    } else if row_menu_button(row, pitch).contains(pos) {
        Some(RowButton::Menu)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Zone {
    Above,
    Row(usize),
    Below,
}

pub(super) fn draw_hidden<R: Copy>(f: &mut Frame, muted: Color, layout: &Rows, rows: &[R], named: impl Fn(R) -> bool) {
    let (above, below) = layout.hidden();
    let count = |range: Range<usize>| (!range.is_empty()).then(|| rows[range].iter().filter(|r| named(**r)).count());
    draw_more(f, muted, [more_above(layout.list), layout.more_below()], count(above), count(below));
}

pub(super) fn draw_landing(f: &mut Frame, r: Rect, indent: Option<u16>) {
    let Some(indent) = indent else { return };
    let row = Rect { height: r.height.min(1), ..r };
    let width = usize::from(row.width.saturating_sub(indent + 1));
    let line = Line::from(vec![Span::raw(" ".repeat(usize::from(indent))), Span::raw("─".repeat(width))]);
    f.render_widget(Paragraph::new(line.style(Style::default().fg(Color::Cyan))), row);
}

pub(super) struct Band {
    pub(super) r: Rect,
    pub(super) pitch: u16,
    pub(super) lead: Vec<Span<'static>>,
    pub(super) bg: Style,
}

impl Band {
    pub(super) fn lead_width(&self) -> usize {
        self.lead.iter().map(Span::width).sum()
    }

    pub(super) fn room(&self) -> usize {
        room(self.r, self.pitch, self.lead_width())
    }
}

pub(super) fn room(r: Rect, pitch: u16, lead: usize) -> usize {
    let buttons = row_close_button(r, pitch).width + row_menu_button(r, pitch).width;
    usize::from(r.width).saturating_sub(lead + usize::from(buttons) + 1)
}

pub(super) struct Tags {
    spans: Vec<Span<'static>>,
    width: usize,
}

impl Tags {
    pub(super) fn fit(tags: Vec<Span<'static>>, room: usize) -> Self {
        let mut fitted = Self { spans: Vec::new(), width: 0 };
        for tag in tags {
            let width = if fitted.is_empty() { tag.width() } else { fitted.width + 1 + tag.width() };
            if width + 1 >= room {
                break;
            }
            if !fitted.is_empty() {
                fitted.spans.push(Span::raw(" "));
            }
            fitted.spans.push(tag);
            fitted.width = width;
        }
        fitted
    }

    pub(super) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub(super) fn place(&self, tag: usize, right: usize) -> Option<(usize, usize)> {
        let span = self.spans.get(2 * tag)?;
        let before: usize = self.spans[..2 * tag].iter().map(Span::width).sum();
        Some((right.saturating_sub(self.width) + before, span.width()))
    }

    pub(super) fn highlight(&mut self, tag: usize, add: Modifier) {
        if let Some(span) = self.spans.get_mut(2 * tag) {
            span.style = span.style.add_modifier(add);
        }
    }

    pub(super) fn reserved(&self) -> usize {
        if self.is_empty() { 0 } else { self.width + 1 }
    }

    pub(super) fn push_onto(self, line: &mut Vec<Span<'static>>, used: usize, room: usize) {
        if self.is_empty() {
            return;
        }
        line.push(Span::raw(" ".repeat(room.saturating_sub(used + self.width))));
        line.extend(self.spans);
    }
}

pub(super) fn draw_more(
    f: &mut Frame,
    muted: Color,
    [top, bottom]: [Rect; 2],
    above: Option<usize>,
    below: Option<usize>,
) {
    let dim = Style::default().fg(muted);
    for (hidden, arrow, r) in [(above, "↑", top), (below, "↓", bottom)] {
        let label = match hidden {
            None => continue,
            Some(0) => format!("  {arrow} more"),
            Some(n) => format!("  {arrow} {n} more"),
        };
        f.render_widget(Paragraph::new(Span::styled(label, dim)), r);
    }
}

pub(super) fn draw_row_buttons(f: &mut Frame, view: &View, row: Rect, pitch: u16, bg: Style) {
    let hover = sidebar_hovered(view, row);
    if !hover && pitch == 1 {
        return;
    }
    let style = |r: Rect, lit: Color| {
        if hover && hovered(view, r) { bg.fg(lit).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) }
    };
    let menu = row_menu_button(row, pitch);
    let menu_style = style(menu, Color::Cyan);
    draw_band(
        f,
        menu,
        Span::styled(format!("{ROW_MENU_ICON:>width$}", width = usize::from(menu.width)), menu_style),
        menu_style,
    );
    let close = row_close_button(row, pitch);
    let close_style = style(close, Color::Red);
    draw_band(f, close, Span::styled(centered("×", close.width), close_style), close_style);
}

pub(super) fn marker(shown: bool) -> Span<'static> {
    Span::styled(if shown { "▌ " } else { "  " }, Style::default().fg(Color::Cyan))
}

pub(super) fn draw_rail(f: &mut Frame, r: Rect) {
    let buf = f.buffer_mut();
    for y in r.top()..r.bottom() {
        let cell = &mut buf[(r.x, y)];
        if cell.symbol() == " " {
            cell.set_symbol("▌").set_fg(Color::Cyan);
        }
    }
}

pub(super) fn arrow(muted: Color, folded: bool) -> Span<'static> {
    Span::styled(if folded { "▸ " } else { "▾ " }, Style::default().fg(muted))
}
