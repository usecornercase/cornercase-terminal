use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

use super::Rows;
use super::widgets::{draw_button, hovered_at};
use crate::queue::State;
use crate::text::truncate_right;

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub(crate) id: u64,
    pub(crate) when: String,
    pub(crate) label: String,
    pub(crate) meta: String,
    pub(crate) status: String,
    pub(crate) state: State,
}

#[derive(Debug, Clone)]
pub(crate) struct View {
    pub(crate) entries: Vec<Row>,
    pub(crate) scroll: usize,
    pub(crate) pending: usize,
}

pub(crate) enum Hit {
    Close,
    New,
    Clear,
    Remove(u64),
    Run(u64),
    Entry(u64),
}

pub(crate) fn rows(area: Rect, view: &View) -> Rows {
    let y = area.y.saturating_add(2).min(area.bottom());
    Rows {
        list: Rect { y, height: area.bottom() - y, ..area },
        heights: vec![5; view.entries.len()],
        button: 1,
        scroll: view.scroll,
    }
}

pub(crate) fn close(area: Rect) -> Rect {
    Rect::new(area.right().saturating_sub(3), area.y, 3, 1).intersection(area)
}

pub(crate) fn clear(area: Rect, view: &View) -> Rect {
    if !view.entries.iter().any(|row| matches!(row.state, State::Started { .. })) {
        return Rect::default();
    }
    Rect::new(close(area).x.saturating_sub(16), area.y + u16::from(area.width < 28), 15, 1).intersection(area)
}

fn remove(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(3), row.y + 1, 3, 1).intersection(row)
}

fn run(row: Rect) -> Rect {
    Rect::new(row.right().saturating_sub(12), row.y + 3, 11, 1).intersection(row)
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if clear(area, view).contains(pos) {
        return Some(Hit::Clear);
    }
    let rows = rows(area, view);
    if rows.button().contains(pos) {
        return Some(Hit::New);
    }
    let i = rows.at(pos)?;
    let row = rows.item(i);
    let entry = &view.entries[i];
    Some(if remove(row).contains(pos) {
        Hit::Remove(entry.id)
    } else if run(row).contains(pos) && !matches!(entry.state, State::Starting | State::Started { .. }) {
        Hit::Run(entry.id)
    } else {
        Hit::Entry(entry.id)
    })
}

pub(super) fn draw(
    f: &mut Frame,
    area: Rect,
    view: &View,
    hover: Option<Position>,
    muted: Color,
    compact: bool,
    light: bool,
) {
    let style = Style::default().fg(muted);
    let title = format!("Queue {}", view.pending);
    draw_button(
        f,
        Rect::new(
            area.x,
            area.y,
            if clear(area, view).is_empty() || area.width < 28 {
                close(area).x.saturating_sub(area.x)
            } else {
                clear(area, view).x.saturating_sub(area.x)
            },
            1,
        )
        .intersection(area),
        "",
        &title,
        style.add_modifier(Modifier::BOLD),
    );
    draw_button(f, close(area), "", "×", style);
    draw_button(f, clear(area, view), "", "clear started", style);
    let rows = rows(area, view);
    for (i, entry) in view.entries.iter().enumerate() {
        let row = rows.item(i);
        if row.is_empty() {
            continue;
        }
        let lit = hovered_at(hover, row);
        if lit {
            f.buffer_mut().set_style(row, Style::default().bg(super::hover_colour(light)));
        }
        let width = usize::from(row.width.saturating_sub(2));
        for (line, text, colour) in [
            (0, &entry.when, muted),
            (1, &entry.label, if light { Color::Black } else { Color::White }),
            (2, &entry.meta, muted),
            (
                3,
                &entry.status,
                match entry.state {
                    State::Failed { .. } => Color::Red,
                    State::Missed => Color::Yellow,
                    State::Started { .. } => Color::Green,
                    State::Pending | State::Starting => muted,
                },
            ),
        ] {
            let reserve = if line == 1 {
                3
            } else if line == 3 && !matches!(entry.state, State::Starting | State::Started { .. }) {
                12
            } else {
                0
            };
            let r = Rect::new(row.x + 1, row.y + line, row.width.saturating_sub(2), 1).intersection(row);
            f.render_widget(
                Paragraph::new(Span::styled(
                    truncate_right(text, width.saturating_sub(reserve)),
                    Style::default().fg(colour),
                )),
                r,
            );
        }
        if lit || compact {
            draw_button(f, remove(row), "", "×", style);
        }
        if !matches!(entry.state, State::Starting | State::Started { .. }) {
            draw_button(f, run(row), "", "start now", Style::default().fg(Color::Cyan));
        }
    }
    let (above, below) = rows.hidden();
    for (r, text) in [
        (super::more_above(rows.list), (!above.is_empty()).then(|| format!(" ↑ {} more", above.len()))),
        (rows.more_below(), (!below.is_empty()).then(|| format!(" ↓ {} more", below.len()))),
    ] {
        if let Some(text) = text {
            f.render_widget(Paragraph::new(Span::styled(text, style)), r);
        }
    }
    draw_button(f, rows.button(), "", "+ New", Style::default().fg(Color::Cyan));
}
