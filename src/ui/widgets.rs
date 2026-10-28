use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::editor::Highlight;

use super::View;

pub(super) fn button_width(label: &str) -> u16 {
    u16::try_from(label.chars().count()).unwrap_or(u16::MAX).saturating_add(2)
}

pub(crate) fn update_button(settings: Rect, label: &str) -> Rect {
    let width = button_width(label).min(settings.width);
    Rect { x: settings.right() - width, width, ..settings }
}

pub(super) fn paint_selection(buf: &mut Buffer, text: Rect, top: usize, marks: &[Highlight]) {
    for mark in marks {
        let Some(row) = mark.line.checked_sub(top).and_then(|row| u16::try_from(row).ok()).filter(|&r| r < text.height)
        else {
            continue;
        };
        let cell = |cells: usize| text.x.saturating_add(u16::try_from(cells).unwrap_or(u16::MAX)).min(text.right());
        let end = if mark.to_edge { text.right() } else { cell(mark.cells.end) };
        let start = cell(mark.cells.start);
        let area = Rect { x: start, y: text.y + row, width: end.saturating_sub(start), height: 1 };
        buf.set_style(area, Style::default().add_modifier(Modifier::REVERSED));
    }
}

pub(super) fn hovered(view: &View, r: Rect) -> bool {
    view.hover.is_some_and(|p| r.contains(p))
}

pub(super) fn fit(texts: &[String], room: usize) -> String {
    let fitting = texts.iter().find(|t| t.chars().count() <= room).or(texts.last());
    crate::text::truncate_right(fitting.map_or("", String::as_str), room)
}

pub(super) fn scroll_edge(body: Rect, (up, down): (bool, bool), y: u16) -> Option<isize> {
    if body.is_empty() {
        None
    } else if y <= body.y && up {
        Some(-1)
    } else if y >= body.bottom() - 1 && down {
        Some(1)
    } else {
        None
    }
}

pub(super) fn dim(muted: Color) -> Style {
    Style::default().fg(muted)
}

pub(super) fn hovered_at(hover: Option<Position>, r: Rect) -> bool {
    hover.is_some_and(|p| r.contains(p))
}

pub(super) fn put(buf: &mut Buffer, at: impl Into<Position>, text: &str, style: Style, end: u16) -> u16 {
    let Position { x, y } = at.into();
    if x >= end {
        return x;
    }
    buf.set_stringn(x, y, text, usize::from(end - x), style).0
}

pub(super) fn draw_close(buf: &mut Buffer, r: Rect, hover: Option<Position>, muted: Color) {
    let style =
        if hovered_at(hover, r) { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { dim(muted) };
    put(buf, (r.x + 1, r.y), "×", style, r.right());
}

pub(super) fn action_style(lit: bool) -> Style {
    if lit {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    }
}

pub(super) fn rail(selected: bool, bg: Style) -> Span<'static> {
    if selected { Span::styled("▌", bg.fg(Color::Cyan)) } else { Span::styled(" ", bg) }
}

pub(super) fn sidebar_hovered(view: &View, r: Rect) -> bool {
    view.overlay.is_none() && view.drag.is_none() && hovered(view, r)
}

pub(super) fn overlay_block(muted: Color, title: &str) -> Block<'_> {
    let border = Style::default().fg(muted);
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(border);
    if title.is_empty() {
        block
    } else {
        block.title(Line::from(vec![
            Span::styled("─", border),
            Span::styled(format!(" {title} "), Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD)),
        ]))
    }
}

pub(super) fn tab_style(active: bool, lit: bool, surface: Color, rest: Color) -> Style {
    if active {
        Style::default().fg(Color::Cyan).bg(surface).add_modifier(Modifier::BOLD)
    } else if lit {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(rest)
    }
}

pub(super) fn middle(r: Rect) -> Rect {
    Rect { y: r.y + r.height.saturating_sub(1) / 2, height: r.height.min(1), ..r }
}

pub(super) fn centered(text: &str, width: u16) -> String {
    format!("{text:^width$}", width = usize::from(width))
}

pub(super) fn draw_band<'a>(f: &mut Frame, r: Rect, line: impl Into<Line<'a>>, style: Style) {
    f.render_widget(Block::new().style(style), r);
    f.render_widget(Paragraph::new(line.into()).style(style), middle(r));
}

pub(super) fn draw_title(f: &mut Frame, muted: Color, r: Rect, title: &str) {
    let style = Style::default().fg(muted).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(format!(" {title}"), style)), middle(r));
}

pub(super) fn draw_separator(f: &mut Frame, colour: Color, r: Rect) {
    let line = "─".repeat(usize::from(r.width.saturating_sub(2)));
    f.render_widget(Paragraph::new(Span::styled(format!(" {line}"), Style::default().fg(colour))), r);
}

pub(super) fn button_style(view: &View, r: Rect, idle: Style, hover_bg: Color) -> Style {
    if sidebar_hovered(view, r) {
        Style::default().fg(Color::Black).bg(hover_bg).add_modifier(Modifier::BOLD)
    } else {
        idle
    }
}

pub(super) fn draw_button(f: &mut Frame, r: Rect, indent: &str, label: &str, style: Style) {
    if r.height > 1
        && let Some(bg) = style.bg
    {
        f.render_widget(Block::new().style(Style::default().bg(bg)), r);
    }
    let line = Line::from(vec![Span::raw(indent), Span::styled(format!(" {label} "), style)]);
    f.render_widget(Paragraph::new(line), middle(r));
}
