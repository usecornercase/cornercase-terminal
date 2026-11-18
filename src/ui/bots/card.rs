use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::agents::activity::Status;
use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::layout::Areas;
use crate::ui::rows::{draw_menu_in, draw_more, menu_shown, row_menu_button};
use crate::ui::theme::status_icon;
use crate::ui::widgets::draw_title;

const LABEL: usize = 9;
const HEADER: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardRow {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) red: bool,
    pub(crate) folder: bool,
    pub(crate) routine: Option<u64>,
    pub(crate) status: Option<Status>,
    pub(crate) pane: Option<u64>,
}

impl CardRow {
    pub(crate) fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), red: false, folder: false, routine: None, status: None, pane: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotCard {
    pub(crate) name: String,
    pub(crate) rows: Vec<CardRow>,
}

fn inner(list: Rect) -> Rect {
    Rect { x: list.x.saturating_add(1), width: list.width.saturating_sub(1), ..list }
}

fn shown_rows(list: Rect, card: &BotCard) -> usize {
    let room = usize::from(list.height.saturating_sub(HEADER));
    if card.rows.len() <= room {
        return card.rows.len();
    }
    let mut shown = room.saturating_sub(1).max(1).min(room);
    while shown > 0 && card.rows.get(shown).is_some_and(|row| row.label.is_empty()) {
        shown -= 1;
    }
    shown
}

pub(crate) fn card_folder(list: Rect, card: &BotCard) -> Rect {
    let list = inner(list);
    let at = card.rows.iter().position(|row| row.folder).unwrap_or(usize::MAX);
    if at >= shown_rows(list, card) {
        return Rect::default();
    }
    card_row(list, at)
}

pub(crate) fn card_row_at(list: Rect, card: &BotCard, pos: Position) -> Option<usize> {
    let list = inner(list);
    (0..shown_rows(list, card)).find(|at| card_row(list, *at).contains(pos))
}

pub(crate) fn card_routine(list: Rect, card: &BotCard, pos: Position) -> Option<u64> {
    card.rows[card_row_at(list, card, pos)?].routine
}

fn routine_span(list: Rect, card: &BotCard, routine: u64) -> Rect {
    (0..shown_rows(list, card))
        .filter(|at| card.rows[*at].routine == Some(routine))
        .map(|at| card_row(list, at))
        .reduce(Rect::union)
        .unwrap_or_default()
}

fn menu_of(list: Rect, card: &BotCard, routine: u64, pitch: u16) -> Rect {
    let span = routine_span(inner(list), card, routine);
    if span.is_empty() { Rect::default() } else { row_menu_button(span, pitch) }
}

#[cfg(test)]
pub(crate) fn card_menu(list: Rect, card: &BotCard, routine: u64, pitch: u16) -> Rect {
    menu_of(list, card, routine, pitch)
}

pub(crate) fn card_buttons(view: &View, list: Rect, pitch: u16) -> Vec<(u64, Rect)> {
    let Some(card) = &view.bot_card else { return Vec::new() };
    let inner = inner(list);
    let mut routines: Vec<u64> = (0..shown_rows(inner, card)).filter_map(|at| card.rows[at].routine).collect();
    routines.dedup();
    routines.retain(|routine| menu_shown(view, routine_span(inner, card, *routine), pitch));
    routines
        .into_iter()
        .map(|routine| (routine, menu_of(list, card, routine, pitch)))
        .filter(|(_, r)| !r.is_empty())
        .collect()
}

fn row_width(buttons: &[(u64, Rect)], card: &BotCard, list: Rect, i: usize, width: usize) -> usize {
    let routine = card.rows[i].routine;
    let first = routine.is_some() && (i == 0 || card.rows[i - 1].routine != routine);
    let row = card_row(list, i);
    let button = buttons.iter().find(|(id, _)| Some(*id) == routine).map(|(_, rect)| rect);
    match button {
        Some(button) if first => usize::from(button.x.saturating_sub(row.x)).saturating_sub(1),
        _ => width,
    }
}

pub(in crate::ui) fn draw_card(f: &mut Frame, view: &View, card: &BotCard, areas: &Areas) {
    draw_title(f, view.muted, areas.workspaces_title, "Bot");
    let list = inner(areas.workspaces_list);
    let width = usize::from(list.width);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let name = Line::from(Span::styled(truncate_right(&card.name, width), bold));
    f.render_widget(Paragraph::new(name), Rect { height: 1, ..list }.intersection(list));
    let shown = shown_rows(list, card);
    let under = view.hover.and_then(|pos| card_routine(areas.workspaces_list, card, pos));
    let buttons = card_buttons(view, areas.workspaces_list, areas.pitch);
    for (i, row) in card.rows.iter().take(shown).enumerate() {
        let target = row.routine.is_some() && row.routine == under;
        let line = card_line(view, row, row_width(&buttons, card, list, i, width), target);
        f.render_widget(Paragraph::new(line), card_row(list, i));
    }
    for (routine, _) in &buttons {
        draw_menu_in(f, view, routine_span(list, card, *routine), areas.pitch, Style::default());
    }
    let hidden = card.rows.len() - shown;
    if hidden > 0 {
        draw_more(f, view.muted, [Rect::default(), card_row(list, shown)], None, Some(hidden));
    }
}

fn card_row(list: Rect, i: usize) -> Rect {
    let y = list.y.saturating_add(HEADER).saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
    Rect { y, height: 1, ..list }.intersection(list)
}

fn card_line(view: &View, row: &CardRow, width: usize, target: bool) -> Line<'static> {
    let icon = row.status.map(|status| status_icon(view.muted, status, view.spin));
    let room = width.saturating_sub(LABEL + icon.as_ref().map_or(0, |_| 2));
    let value = if row.folder { truncate_left(&row.value, room) } else { truncate_right(&row.value, room) };
    let style = if row.red {
        Style::default().fg(Color::Red)
    } else if row.folder || target {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(view.soft)
    };
    let mut spans = vec![Span::styled(format!("{:<LABEL$}", row.label), Style::default().fg(view.muted))];
    if let Some(icon) = icon {
        spans.extend([icon, Span::raw(" ")]);
    }
    spans.push(Span::styled(value, style));
    Line::from(spans)
}
