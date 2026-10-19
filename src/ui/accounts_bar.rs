use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::View;
use super::layout::Areas;
use super::widgets::button_style;
use crate::text::{cells, truncate_cells};

const ARROW: &str = " ▾";

const MIN_NAME: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pill {
    pub(crate) agent: String,
    pub(crate) account: String,
}

fn full(pill: &Pill) -> String {
    format!(" {}: {}{ARROW} ", pill.agent, pill.account)
}

fn short(pill: &Pill) -> String {
    format!(" {}{ARROW} ", pill.account)
}

fn width(text: &str) -> u16 {
    u16::try_from(cells(text)).unwrap_or(u16::MAX)
}

fn fitting(texts: impl IntoIterator<Item = String>, room: u16) -> Vec<String> {
    let mut used = 0u16;
    let mut kept = Vec::new();
    for text in texts {
        let wanted = width(&text).saturating_add(u16::from(!kept.is_empty()));
        if used.saturating_add(wanted) > room {
            break;
        }
        used += wanted;
        kept.push(text);
    }
    kept
}

pub(crate) fn pills(areas: &Areas, pills: &[Pill]) -> Vec<(usize, Rect, String)> {
    let bar = areas.accounts;
    let row = Rect { y: bar.bottom().saturating_sub(1), height: bar.height.min(1), ..bar };
    if row.is_empty() || pills.is_empty() {
        return Vec::new();
    }
    let room = row.width.saturating_sub(1);
    let mut texts = fitting(pills.iter().map(full), room);
    if texts.len() < pills.len() {
        texts = fitting(pills.iter().map(short), room);
    }
    if texts.is_empty() {
        let name = usize::from(room).saturating_sub(cells(ARROW) + 2);
        if name < MIN_NAME {
            return Vec::new();
        }
        texts.push(format!(" {}{ARROW} ", truncate_cells(&pills[0].account, name)));
    }
    let mut x = row.x + 1;
    texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let r = Rect { x, width: width(&text), ..row };
            x = r.right() + 1;
            (i, r, text)
        })
        .collect()
}

pub(crate) fn hit(areas: &Areas, shown: &[Pill], pos: Position) -> Option<usize> {
    pills(areas, shown).into_iter().find_map(|(i, r, _)| r.contains(pos).then_some(i))
}

pub(super) fn draw(f: &mut Frame, view: &View, areas: &Areas) {
    if areas.accounts.is_empty() || view.accounts.is_empty() {
        return;
    }
    let bar = Style::default().fg(view.muted);
    let top = Rect { height: 1, ..areas.accounts };
    f.render_widget(Block::default().borders(Borders::TOP).border_style(Style::default().fg(view.line())), top);
    for (_, r, text) in pills(areas, &view.accounts) {
        let style = button_style(view, r, bar, Color::Cyan);
        f.render_widget(Paragraph::new(text).style(style), r);
    }
}
