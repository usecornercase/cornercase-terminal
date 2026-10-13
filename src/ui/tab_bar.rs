use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{
    Details, Drag, ICON_WIDTH, ROW_MENU_ICON, TabEntry, View, button_style, centered, draw_band, draw_details, hovered,
};
use crate::text::truncate_right;

pub(crate) const HEIGHT: u16 = 2;
const MAX_NAME: usize = 24;
const CLOSE_WIDTH: u16 = 3;
const MENU_WIDTH: u16 = 2;
const BUTTON_WIDTH: u16 = 3;
const NEW_LABEL: &str = "+";
const LEFT_LABEL: &str = "‹";
const RIGHT_LABEL: &str = "›";

pub(crate) struct TabBar {
    pub(crate) tabs: Vec<TabEntry>,
    pub(crate) active: Option<usize>,
    pub(crate) details: Details,
    pub(crate) scroll: usize,
}

impl TabBar {
    pub(crate) fn strip(&self, bar: Rect) -> Strip {
        Strip::new(bar, &self.tabs, self.scroll)
    }
}

pub(crate) fn width(tab: &TabEntry) -> u16 {
    let name = tab.name.chars().count().min(MAX_NAME);
    let others = others(tab).map_or(0, |n| 1 + n.chars().count());
    u16::try_from(1 + ICON_WIDTH + name + others).unwrap_or(u16::MAX).saturating_add(MENU_WIDTH + CLOSE_WIDTH)
}

fn others(tab: &TabEntry) -> Option<String> {
    (tab.others > 0).then(|| format!("+{}", tab.others))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Strip {
    row: Rect,
    widths: Vec<u16>,
    scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Tab(usize),
    Close(usize),
    Menu(usize),
    New,
    Scroll(isize),
}

impl Strip {
    pub(crate) fn new(bar: Rect, tabs: &[TabEntry], scroll: usize) -> Self {
        Self { row: Rect { height: bar.height.min(1), ..bar }, widths: tabs.iter().map(width).collect(), scroll }
    }

    fn total(&self, range: Range<usize>) -> u32 {
        self.widths[range].iter().map(|&w| u32::from(w)).sum()
    }

    fn overflows(&self) -> bool {
        self.total(0..self.widths.len()) + u32::from(BUTTON_WIDTH) > u32::from(self.row.width)
    }

    fn start(&self) -> u16 {
        if self.overflows() { self.row.x.saturating_add(BUTTON_WIDTH) } else { self.row.x }
    }

    fn room(&self) -> u16 {
        let buttons = if self.overflows() { 3 * BUTTON_WIDTH } else { BUTTON_WIDTH };
        self.row.width.saturating_sub(buttons)
    }

    fn fitting_before(&self, end: usize) -> usize {
        let room = u32::from(self.room());
        let mut start = end;
        while start > 0 && (start == end || self.total(start - 1..end) <= room) {
            start -= 1;
        }
        start
    }

    fn first(&self) -> usize {
        self.scroll.min(self.fitting_before(self.widths.len()))
    }

    fn end(&self) -> usize {
        let (first, room) = (self.first(), u32::from(self.room()));
        let fit = (first..self.widths.len()).take_while(|&i| i == first || self.total(first..i + 1) <= room).last();
        fit.map_or(first, |i| i + 1)
    }

    fn tabs_area(&self) -> Rect {
        Rect { x: self.start(), width: self.room(), ..self.row }.intersection(self.row)
    }

    pub(crate) fn item(&self, i: usize) -> Rect {
        let first = self.first();
        if !(first..self.end()).contains(&i) {
            return Rect::default();
        }
        let x = u32::from(self.start()) + self.total(first..i);
        let x = u16::try_from(x).unwrap_or(u16::MAX);
        Rect { x, width: self.widths[i], ..self.row }.intersection(self.tabs_area())
    }

    pub(crate) fn close(&self, i: usize) -> Rect {
        let r = self.item(i);
        Rect { x: r.right().saturating_sub(CLOSE_WIDTH), width: CLOSE_WIDTH.min(r.width), ..r }
    }

    pub(crate) fn menu(&self, i: usize) -> Rect {
        let close = self.close(i);
        let x = close.x.saturating_sub(MENU_WIDTH).max(self.item(i).x);
        Rect { x, width: close.x - x, ..close }
    }

    pub(crate) fn at(&self, pos: Position) -> Option<usize> {
        (self.first()..self.end()).find(|&i| self.item(i).contains(pos))
    }

    pub(crate) fn new_button(&self) -> Rect {
        let x = if self.overflows() {
            self.row.right().saturating_sub(BUTTON_WIDTH)
        } else {
            let end = u32::from(self.start()) + self.total(self.first()..self.end());
            u16::try_from(end).unwrap_or(u16::MAX)
        };
        Rect { x, width: BUTTON_WIDTH, ..self.row }.intersection(self.row)
    }

    pub(crate) fn left(&self) -> Rect {
        if !self.overflows() {
            return Rect::default();
        }
        Rect { width: BUTTON_WIDTH, ..self.row }.intersection(self.row)
    }

    pub(crate) fn right(&self) -> Rect {
        if !self.overflows() {
            return Rect::default();
        }
        Rect { x: self.tabs_area().right(), width: BUTTON_WIDTH, ..self.row }.intersection(self.row)
    }

    pub(crate) fn hidden(&self) -> (bool, bool) {
        (self.first() > 0, self.end() < self.widths.len())
    }

    pub(crate) fn scrolled(&self, delta: isize) -> usize {
        let max = self.fitting_before(self.widths.len());
        self.first().saturating_add_signed(delta).min(max)
    }

    pub(crate) fn reveal(&self, i: usize) -> usize {
        let first = self.first();
        if i >= self.widths.len() || (first..self.end()).contains(&i) {
            first
        } else if i < first {
            i
        } else {
            self.fitting_before(i + 1)
        }
    }

    pub(crate) fn edge(&self, pos: Position) -> Option<isize> {
        let (before, after) = self.hidden();
        if before && self.left().contains(pos) {
            Some(-1)
        } else if after && self.right().contains(pos) {
            Some(1)
        } else {
            None
        }
    }

    pub(crate) fn hit(&self, pos: Position) -> Option<Hit> {
        if self.new_button().contains(pos) {
            return Some(Hit::New);
        }
        if self.left().contains(pos) {
            return Some(Hit::Scroll(-1));
        }
        if self.right().contains(pos) {
            return Some(Hit::Scroll(1));
        }
        let i = self.at(pos)?;
        Some(if self.close(i).contains(pos) {
            Hit::Close(i)
        } else if self.menu(i).contains(pos) {
            Hit::Menu(i)
        } else {
            Hit::Tab(i)
        })
    }

    pub(crate) fn drop(&self, dragged: usize, pos: Position) -> Option<usize> {
        if self.left().contains(pos) {
            return Some(self.first());
        }
        if !self.row.contains(pos) {
            return None;
        }
        match self.at(pos) {
            Some(i) if i == dragged => None,
            Some(i) if i < dragged => Some(i),
            Some(i) => Some(i + 1),
            None => Some(self.end()),
        }
    }

    fn landing(&self, before: usize) -> Rect {
        let (first, end) = (self.first(), self.end());
        let x = if (first..end).contains(&before) {
            self.item(before).x
        } else if before == end && end > first {
            self.item(end - 1).right().saturating_sub(1)
        } else {
            return Rect::default();
        };
        Rect { x, width: 1, ..self.row }.intersection(self.row)
    }
}

pub(crate) fn draw(f: &mut Frame, view: &View, bar: &TabBar, r: Rect) {
    let strip = bar.strip(r);
    let dragged = match view.drag {
        Some(Drag::Bar(t, landing)) => Some((t, landing)),
        _ => None,
    };
    for i in 0..bar.tabs.len() {
        let item = strip.item(i);
        if item.is_empty() {
            continue;
        }
        let active = bar.active == Some(i);
        let lifted = dragged.is_some_and(|(t, _)| t == i);
        let marked = active || lifted;
        let bg = view.row_background(item, marked);
        draw_tab(f, view, &bar.tabs[i], (item, bg), marked);
        if view.row_hovered(item) {
            draw_tab_buttons(f, view, (strip.menu(i), strip.close(i)), bg);
        }
    }
    let (before, after) = strip.hidden();
    draw_button(f, view, strip.left(), LEFT_LABEL, before);
    draw_button(f, view, strip.right(), RIGHT_LABEL, after);
    let new = strip.new_button();
    let accent = Style::default().fg(Color::Cyan);
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {NEW_LABEL} "), button_style(view, new, accent, Color::Cyan))),
        new,
    );
    if let Some((_, Some(landing))) = dragged {
        let line = strip.landing(landing);
        if !line.is_empty() {
            let style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
            f.buffer_mut().set_string(line.x, line.y, "│", style);
        }
    }
    if bar.details.lines() > 1 {
        draw_details(f, view.muted, (r, Rect::default()), &bar.details, None, 1);
    }
}

fn draw_tab(f: &mut Frame, view: &View, tab: &TabEntry, (r, bg): (Rect, Style), marked: bool) {
    let style = if marked { bg.fg(Color::White).add_modifier(Modifier::BOLD) } else { bg.fg(Color::Gray) };
    let mut line = vec![Span::styled(" ", bg)];
    let mut used = 1;
    line.extend(tab.prefix(view.muted, bg, view.spin));
    used += ICON_WIDTH;
    let others = others(tab);
    let reserved = others.as_ref().map_or(0, |n| 1 + n.chars().count());
    let room =
        usize::from(r.width).saturating_sub(used + reserved + usize::from(MENU_WIDTH + CLOSE_WIDTH)).min(MAX_NAME);
    line.push(Span::styled(truncate_right(&tab.name, room), style));
    if let Some(n) = others {
        line.extend([Span::styled(" ", bg), Span::styled(n, bg.fg(view.muted))]);
    }
    f.render_widget(Paragraph::new(Line::from(line)).style(bg), r);
}

fn draw_tab_buttons(f: &mut Frame, view: &View, (menu, close): (Rect, Rect), bg: Style) {
    let style = |r: Rect, lit: Color| {
        if hovered(view, r) { bg.fg(lit).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) }
    };
    let menu_style = style(menu, Color::Cyan);
    let menu_text = format!("{ROW_MENU_ICON:>width$}", width = usize::from(menu.width));
    draw_band(f, menu, Span::styled(menu_text, menu_style), menu_style);
    let close_style = style(close, Color::Red);
    draw_band(f, close, Span::styled(centered("×", close.width), close_style), close_style);
}

fn draw_button(f: &mut Frame, view: &View, r: Rect, label: &str, more: bool) {
    if r.is_empty() {
        return;
    }
    let idle = Style::default().fg(if more { Color::Cyan } else { view.muted });
    let style = if more { button_style(view, r, idle, Color::Cyan) } else { idle };
    f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), r);
}

#[cfg(test)]
mod tests;
