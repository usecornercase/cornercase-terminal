use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::agents::activity::Status;
use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::layout::Areas;
use crate::ui::rows::draw_more;
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

pub(in crate::ui) fn draw_card(f: &mut Frame, view: &View, card: &BotCard, areas: &Areas) {
    draw_title(f, view.muted, areas.workspaces_title, "Bot");
    let list = inner(areas.workspaces_list);
    let width = usize::from(list.width);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let name = Line::from(Span::styled(truncate_right(&card.name, width), bold));
    f.render_widget(Paragraph::new(name), Rect { height: 1, ..list }.intersection(list));
    let shown = shown_rows(list, card);
    let under = view.hover.and_then(|pos| card_routine(areas.workspaces_list, card, pos));
    for (i, row) in card.rows.iter().take(shown).enumerate() {
        let target = row.routine.is_some() && row.routine == under;
        f.render_widget(Paragraph::new(card_line(view, row, width, target)), card_row(list, i));
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
