use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use super::View;
use super::chrome::{BACK_LABEL, draw_back};
use super::layout::{Areas, bots_pad};
use super::rows::{Rows, Tags, draw_hidden, draw_menu_in, draw_rail, marker, row_menu_button};
use super::sidebar::draw_new_project;
use super::theme::{BRAND_COLOR, WAITING_COLOR, status_icon};
use super::widgets::{button_style, button_width, draw_band, draw_button, draw_title, middle, sidebar_hovered};
use crate::agents::activity::Status;

pub(crate) mod agent;
pub(crate) mod card;
pub(crate) mod chat;
pub(crate) mod lobby;
pub(crate) mod run;

pub(crate) const BOTS_LABEL: &str = "Bots ›";

pub(super) const BOTS_SHORT: &str = "Bots";

const TITLE: &str = "Bots";

const MIN_NAME_WIDTH: usize = 4;

const ASKS: &str = "asks";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BotEntry {
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) fallback: String,
    pub(crate) thinking: bool,
    pub(crate) unread: usize,
    pub(crate) active: bool,
    pub(crate) asking: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BotsView {
    pub(crate) entries: Vec<BotEntry>,
    pub(crate) scroll: usize,
    pub(crate) page: bool,
}

impl BotsView {
    fn thinking(&self) -> bool {
        self.entries.iter().any(|entry| entry.thinking)
    }

    fn unread(&self) -> usize {
        self.entries.iter().map(|entry| entry.unread).sum()
    }

    pub(super) fn marks(&self) -> u16 {
        marks_width(self.thinking(), self.unread())
    }

    pub(super) fn lobby(&self) -> lobby::Lobby {
        if self.entries.is_empty() { lobby::Lobby::Empty } else { lobby::Lobby::Pick }
    }
}

pub(crate) fn bot_rows(list: Rect, pitch: u16, count: usize, scroll: usize) -> Rows {
    Rows { list, heights: vec![pitch.max(1); count], button: pitch.max(1), scroll }
}

#[cfg(test)]
pub(crate) fn bots_new(list: Rect, pitch: u16, count: usize, scroll: usize) -> Rect {
    bot_rows(list, pitch, count, scroll).button()
}

pub(crate) fn bot_menu(row: Rect, pitch: u16) -> Rect {
    row_menu_button(row, pitch)
}

pub(crate) fn marks_width(thinking: bool, unread: usize) -> u16 {
    let spin = if thinking { 2 } else { 0 };
    let count = if unread == 0 { 0 } else { unread_text(unread).chars().count() + 1 };
    spin + u16::try_from(count).unwrap_or(u16::MAX)
}

fn unread_text(count: usize) -> String {
    if count > 9 { String::from("●9+") } else { format!("●{count}") }
}

fn unread_span(count: usize) -> Span<'static> {
    Span::styled(unread_text(count), Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD))
}

pub(super) fn draw_bots(f: &mut Frame, view: &View, bots: &BotsView, areas: &Areas) {
    if !areas.bots_button.is_empty() {
        draw_bots_button(f, view, bots, (areas.bots_button, areas.compact()));
    }
    if areas.bots.is_empty() {
        return;
    }
    draw_header(f, view, areas);
    let rows = bot_rows(areas.bots_list, areas.pitch, bots.entries.len(), bots.scroll);
    for (i, entry) in bots.entries.iter().enumerate() {
        let r = rows.item(i);
        if !r.is_empty() {
            draw_bot(f, view, r, areas.pitch, entry);
        }
    }
    let indices: Vec<usize> = (0..bots.entries.len()).collect();
    draw_hidden(f, view.muted, &rows, &indices, |_| true);
    draw_new_project(f, view, rows.button());
}

fn draw_header(f: &mut Frame, view: &View, areas: &Areas) {
    if areas.compact() {
        draw_back(f, view, areas, TITLE);
        return;
    }
    let back = areas.back;
    draw_button(f, back, "", BACK_LABEL, button_style(view, back, Style::default().fg(Color::Cyan), Color::Cyan));
    let rest = Rect { x: back.right(), width: areas.bots_title.right().saturating_sub(back.right()), ..back };
    draw_title(f, view.muted, rest, TITLE);
}

fn draw_bots_button(f: &mut Frame, view: &View, bots: &BotsView, (r, compact): (Rect, bool)) {
    let hovered = sidebar_hovered(view, r);
    let style = button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan);
    if hovered && r.height > 1 {
        f.render_widget(Block::new().style(style), r);
    }
    let lit = |span: Span<'static>| if hovered { Span::styled(span.content, style) } else { span };
    let mut spans = vec![Span::styled(" ", style)];
    let full = button_width(BOTS_LABEL) + bots_pad(compact);
    if r.width >= full + bots.marks() {
        if bots.thinking() {
            spans.extend([lit(status_icon(view.muted, Status::Working, view.spin)), Span::styled(" ", style)]);
        }
        if bots.unread() > 0 {
            spans.extend([lit(unread_span(bots.unread())), Span::styled(" ", style)]);
        }
    }
    let label = if r.width >= full { BOTS_LABEL } else { BOTS_SHORT };
    spans.push(Span::styled(format!("{label} "), style));
    f.render_widget(Paragraph::new(Line::from(spans)), middle(r));
}

fn name_room(entry: &BotEntry, room: usize) -> usize {
    let kept = room.saturating_sub(ASKS.len() + 2);
    if entry.asking && kept >= MIN_NAME_WIDTH { kept } else { room }
}

fn detail_tags(view: &View, entry: &BotEntry, room: usize) -> Tags {
    let waiting = if entry.asking { WAITING_COLOR } else { view.muted };
    let mut steps = vec![(entry.detail.as_str(), waiting)];
    if entry.asking {
        steps.push((ASKS, WAITING_COLOR));
    }
    steps.push((entry.fallback.as_str(), view.muted));
    steps
        .into_iter()
        .map(|(text, colour)| Tags::fit(vec![Span::styled(text.to_string(), Style::default().fg(colour))], room))
        .find(|tags| !tags.is_empty())
        .unwrap_or_else(|| Tags::fit(Vec::new(), room))
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
    let name = crate::text::truncate_cells(&entry.name, name_room(entry, room));
    let used = crate::text::cells(&name);
    line.push(Span::styled(name, Style::default().fg(colour)));
    let tags = detail_tags(view, entry, room.saturating_sub(used.max(MIN_NAME_WIDTH)));
    tags.push_onto(&mut line, used, room);
    draw_band(f, r, Line::from(line), bg);
    draw_menu_in(f, view, r, pitch, bg);
    if entry.active {
        draw_rail(f, r);
    }
}
