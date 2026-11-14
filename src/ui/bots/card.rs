use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::text::{truncate_left, truncate_right};
use crate::ui::View;
use crate::ui::layout::Areas;
use crate::ui::widgets::draw_title;

const LABEL: usize = 9;
const HEADER: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CardRow {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) red: bool,
    pub(crate) folder: bool,
}

impl CardRow {
    pub(crate) fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self { label, value: value.into(), red: false, folder: false }
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

pub(crate) fn card_folder(list: Rect, card: &BotCard) -> Rect {
    let list = inner(list);
    let at = card.rows.iter().position(|row| row.folder).unwrap_or(usize::MAX);
    let y = list.y.saturating_add(HEADER).saturating_add(u16::try_from(at).unwrap_or(u16::MAX));
    Rect { y, height: 1, ..list }.intersection(list)
}

pub(in crate::ui) fn draw_card(f: &mut Frame, view: &View, card: &BotCard, areas: &Areas) {
    draw_title(f, view.muted, areas.workspaces_title, "Bot");
    let list = inner(areas.workspaces_list);
    let width = usize::from(list.width);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let name = Line::from(Span::styled(truncate_right(&card.name, width), bold));
    f.render_widget(Paragraph::new(name), Rect { height: 1, ..list }.intersection(list));
    for (i, row) in card.rows.iter().enumerate() {
        let y = list.y.saturating_add(HEADER).saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
        let r = Rect { y, height: 1, ..list }.intersection(list);
        if r.is_empty() {
            break;
        }
        f.render_widget(Paragraph::new(card_line(view, row, width)), r);
    }
}

fn card_line(view: &View, row: &CardRow, width: usize) -> Line<'static> {
    let room = width.saturating_sub(LABEL);
    let value = if row.folder { truncate_left(&row.value, room) } else { truncate_right(&row.value, room) };
    let style = if row.red {
        Style::default().fg(Color::Red)
    } else if row.folder {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(view.soft)
    };
    Line::from(vec![
        Span::styled(format!("{:<LABEL$}", row.label), Style::default().fg(view.muted)),
        Span::styled(value, style),
    ])
}
