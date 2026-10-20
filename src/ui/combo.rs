use ratatui::Frame;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::View;
use super::dialog::pick_background;
use super::widgets::{hovered, overlay_block, rail};
use crate::list::{COMBO_ROWS, Combo};
use crate::text::truncate_right;

pub(crate) const ARROW: &str = "▾";

pub(crate) fn list(field: Rect, combo: &Combo, screen: Rect) -> Rect {
    let rows = u16::try_from(combo.shown().len().clamp(1, COMBO_ROWS)).unwrap_or(1);
    let height = rows.saturating_add(2);
    let below = field.bottom().saturating_add(height) <= screen.bottom();
    let above = field.y.saturating_sub(screen.y) >= height;
    let y = if below || !above { field.bottom() } else { field.y - height };
    Rect::new(field.x.saturating_sub(1), y, field.width.saturating_add(2), height).intersection(screen)
}

pub(crate) fn rows(list: Rect) -> usize {
    usize::from(list.height.saturating_sub(2))
}

fn item(list: Rect, row: usize) -> Rect {
    let inner = list.inner(Margin::new(1, 1));
    let y = inner.y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    Rect::new(inner.x, y, inner.width, 1).intersection(inner)
}

pub(crate) fn hit(list: Rect, combo: &Combo, pos: Position) -> Option<usize> {
    let shown = combo.shown();
    (0..rows(list)).find(|row| item(list, *row).contains(pos)).and_then(|row| shown.get(combo.scroll() + row).copied())
}

#[derive(Clone, Copy)]
pub(crate) struct Field<'a> {
    pub(crate) combo: &'a Combo,
    pub(crate) placeholder: &'a str,
    pub(crate) focused: bool,
}

pub(crate) fn draw_field(f: &mut Frame, view: &View, field: Rect, state: Field) {
    let Field { combo, placeholder, focused } = state;
    if field.is_empty() {
        return;
    }
    let bg = Style::default().bg(view.surface());
    let text = bg.fg(if view.light { Color::Black } else { Color::White }).add_modifier(Modifier::BOLD);
    let room = usize::from(field.width).saturating_sub(4);
    let typing = combo.is_open() && !combo.query().is_empty();
    let (label, style) = match combo.label() {
        _ if typing => (combo.query(), text),
        Some(label) => (label, text),
        None => (placeholder, bg.fg(view.muted)),
    };
    let label = truncate_right(label, room);
    let gap = room.saturating_sub(label.chars().count());
    let lit = focused || combo.is_open() || hovered(view, field);
    let arrow = bg.fg(if lit { Color::Cyan } else { view.muted });
    let line = Line::from(vec![
        Span::styled(" ", bg),
        Span::styled(label.clone(), style),
        Span::styled(" ".repeat(gap + 1), bg),
        Span::styled(ARROW, arrow),
        Span::styled(" ", bg),
    ]);
    f.render_widget(Paragraph::new(line).style(bg), field);
    if typing && focused {
        let x = field.x.saturating_add(1).saturating_add(u16::try_from(label.chars().count()).unwrap_or(u16::MAX));
        f.set_cursor_position(Position::new(x.min(field.right().saturating_sub(1)), field.y));
    }
}

pub(crate) fn draw_list(f: &mut Frame, view: &View, list: Rect, combo: &Combo) {
    if list.is_empty() {
        return;
    }
    f.render_widget(Clear, list);
    f.render_widget(overlay_block(view.muted, ""), list);
    let shown = combo.shown();
    let width = usize::from(list.width.saturating_sub(3));
    if shown.is_empty() {
        let r = item(list, 0);
        f.render_widget(Paragraph::new(Span::styled(" no match", Style::default().fg(view.muted))), r);
        return;
    }
    let visible = rows(list);
    for row in 0..visible {
        let Some(&i) = shown.get(combo.scroll() + row) else { break };
        let r = item(list, row);
        let selected = combo.highlighted() == Some(i);
        let bg = pick_background(view, selected, r);
        let text = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
        let line = Line::from(vec![rail(selected, bg), Span::styled(truncate_right(&combo.options()[i], width), text)]);
        f.render_widget(Paragraph::new(line).style(bg), r);
    }
    let dim = Style::default().fg(view.muted);
    let above = combo.scroll();
    let below = shown.len().saturating_sub(combo.scroll() + visible);
    for (count, arrow, y) in [(above, "↑", list.y), (below, "↓", list.bottom().saturating_sub(1))] {
        if count > 0 {
            let r = Rect::new(list.x.saturating_add(2), y, list.width.saturating_sub(4), 1);
            f.render_widget(Paragraph::new(Span::styled(format!(" {arrow} {count} more "), dim)), r);
        }
    }
}
