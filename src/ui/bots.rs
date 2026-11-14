use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::chrome::draw_back;
use super::layout::Areas;
use super::rows::{Rows, Tags, arrow, draw_hidden, draw_menu_in, draw_rail, marker, row_close_button, row_menu_button};
use super::theme::{BRAND_COLOR, status_icon};
use super::widgets::{button_style, draw_band, draw_button, draw_separator, middle};
use crate::agents::activity::Status;

pub(crate) mod card;
pub(crate) mod chat;

pub(crate) const BOTS_LABEL: &str = "Bots ›";

const TITLE: &str = "Bots";

const NEW_WIDTH: u16 = 3;

const MIN_NAME_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotEntry {
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) thinking: bool,
    pub(crate) unread: usize,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BotsView {
    pub(crate) entries: Vec<BotEntry>,
    pub(crate) scroll: usize,
    pub(crate) folded: bool,
}

pub(crate) fn bot_rows(list: Rect, pitch: u16, count: usize, scroll: usize) -> Rows {
    Rows { list, heights: vec![pitch.max(1); count], button: 0, scroll }
}

pub(crate) fn bots_new(title: Rect) -> Rect {
    let width = NEW_WIDTH.min(title.width);
    Rect { x: title.right() - width, width, ..title }
}

pub(crate) fn bots_fold(title: Rect) -> Rect {
    Rect { width: title.width.saturating_sub(NEW_WIDTH), ..title }
}

pub(crate) fn bot_menu(row: Rect, pitch: u16) -> Rect {
    row_menu_button(row, pitch)
}

fn unread_text(count: usize) -> String {
    if count > 9 { String::from("●9+") } else { format!("●{count}") }
}

fn unread_span(count: usize) -> Span<'static> {
    Span::styled(unread_text(count), Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD))
}

pub(super) fn draw_bots(f: &mut Frame, view: &View, bots: &BotsView, areas: &Areas) {
    if areas.bots.is_empty() {
        draw_bots_button(f, view, areas);
        return;
    }
    if areas.compact() {
        draw_back(f, view, areas, TITLE);
        draw_new(f, view, areas.bots_title);
    } else {
        draw_separator(f, view.line(), areas.bots_line);
        draw_title_row(f, view, bots, areas.bots_title);
    }
    if bots.folded && !areas.compact() {
        return;
    }
    let rows = bot_rows(areas.bots_list, areas.pitch, bots.entries.len(), bots.scroll);
    for (i, entry) in bots.entries.iter().enumerate() {
        let r = rows.item(i);
        if !r.is_empty() {
            draw_bot(f, view, r, areas.pitch, entry);
        }
    }
    let indices: Vec<usize> = (0..bots.entries.len()).collect();
    draw_hidden(f, view.muted, &rows, &indices, |_| true);
}

fn draw_title_row(f: &mut Frame, view: &View, bots: &BotsView, title: Rect) {
    let muted = Style::default().fg(view.muted).add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::raw(" "), arrow(view.muted, bots.folded), Span::styled(TITLE, muted)];
    let new = bots_new(title);
    let tags = if bots.folded { folded_tags(view, bots) } else { Vec::new() };
    let room = usize::from(title.width.saturating_sub(new.width));
    let used: usize = spans.iter().map(Span::width).sum();
    let tags = Tags::fit(tags, room.saturating_sub(used));
    tags.push_onto(&mut spans, used, room);
    f.render_widget(Paragraph::new(Line::from(spans)), middle(title));
    draw_new(f, view, title);
}

fn draw_new(f: &mut Frame, view: &View, title: Rect) {
    let new = bots_new(title);
    let style = button_style(view, new, Style::default().fg(Color::Cyan), Color::Cyan);
    draw_button(f, new, "", "+", style);
}

fn draw_bots_button(f: &mut Frame, view: &View, areas: &Areas) {
    if !areas.bots_button.is_empty() {
        let style = button_style(view, areas.bots_button, Style::default().fg(Color::Cyan), Color::Cyan);
        draw_button(f, areas.bots_button, "", BOTS_LABEL, style);
    }
}

fn folded_tags(view: &View, bots: &BotsView) -> Vec<Span<'static>> {
    let mut tags = Vec::new();
    if bots.entries.iter().any(|entry| entry.thinking) {
        tags.push(status_icon(view.muted, Status::Working, view.spin));
    }
    let unread: usize = bots.entries.iter().map(|entry| entry.unread).sum();
    if unread > 0 {
        tags.push(unread_span(unread));
    }
    tags
}

fn draw_bot(f: &mut Frame, view: &View, r: Rect, pitch: u16, entry: &BotEntry) {
    let bg = view.row_background(r, entry.active);
    let mut line = vec![marker(entry.active)];
    let mark = if entry.thinking {
        Some(status_icon(view.muted, Status::Working, view.spin))
    } else {
        (entry.unread > 0).then(|| unread_span(entry.unread))
    };
    if let Some(mark) = mark {
        line.extend([mark, Span::raw(" ")]);
    }
    let indent: usize = line.iter().map(Span::width).sum();
    let reserved = r.right().saturating_sub(row_menu_button(r, pitch).x);
    let room = usize::from(r.width).saturating_sub(indent + usize::from(reserved) + 1);
    let colour = if entry.active { Color::Reset } else { view.soft };
    let name = crate::text::truncate_right(&entry.name, room);
    let used = name.chars().count();
    line.push(Span::styled(name, Style::default().fg(colour)));
    let detail = vec![Span::styled(entry.detail.clone(), Style::default().fg(view.muted))];
    let tags = Tags::fit(detail, room.saturating_sub(used.max(MIN_NAME_WIDTH)));
    tags.push_onto(&mut line, used, room);
    draw_band(f, r, Line::from(line), bg);
    draw_menu_in(f, view, r, pitch, bg);
    if entry.active {
        draw_rail(f, r);
    }
}
