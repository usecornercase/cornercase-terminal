use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::run::{RunBlock, run_rows};
use crate::agents::activity::Status;
use crate::editor::{Editor, wrap_text};
use crate::text::{plural, truncate_right};
use crate::ui::View;
use crate::ui::theme::status_icon;
use crate::ui::widgets::{EditorField, button_style, draw_button, draw_editor, first_line, middle};

pub(crate) const EXAMPLES: [&str; 4] = [
    "Which of my open pull requests are blocked, and why?",
    "What did my agents get done today, in every project?",
    "Which issues labelled bug came in today, in every project?",
    "Every weekday at 9, tell me which of my PRs are blocked.",
];

pub(crate) const INTRO: [&str; 2] = [
    "A bot is an agent of your own. It belongs to no project and keeps a memory.",
    "It answers here when you write to it, and on a schedule it sets itself. Try:",
];

pub(crate) const STEPS_SHOWN: usize = 3;

const MAX_INPUT: u16 = 6;
const MARGIN: u16 = 1;
const SEND: &str = "send";
const SEND_WIDTH: u16 = 6;
const NEW_CONVERSATION: &str = "new conversation";
const NEW_CONVERSATION_WIDTH: u16 = 18;
const ROUTINES: &str = "routines";
const ROUTINES_WIDTH: u16 = 10;
const NAME_ROOM: u16 = 12;
const MENU_WIDTH: u16 = 3;
const NEWS: &str = "↓ new";
const NEWS_WIDTH: u16 = 7;
const STOP_WIDTH: u16 = 6;
const BUBBLE_SHARE: usize = 80;
const MIN_BUBBLE: usize = 8;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tone {
    pub(crate) muted: Color,
    pub(crate) surface: Color,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowHit {
    Unfold(u64),
    Retry(u64),
    RunAgain(u64),
    Example(usize),
    Link,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatRow {
    pub(crate) line: Line<'static>,
    pub(crate) hit: Option<(RowHit, Range<u16>)>,
}

impl ChatRow {
    pub(super) fn plain(line: Line<'static>) -> Self {
        Self { line, hit: None }
    }

    fn whole(line: Line<'static>, hit: RowHit) -> Self {
        Self { line, hit: Some((hit, 0..u16::MAX)) }
    }

    fn blank() -> Self {
        Self::plain(Line::default())
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ChatEntry {
    You { text: String, stamp: String },
    Bot { head: String, body: Vec<Line<'static>> },
    Steps { first: u64, texts: Vec<String>, open: bool },
    Error { id: u64, text: String, retry: bool },
    Notice(String),
    Run(RunBlock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Thinking {
    pub(crate) text: String,
    pub(crate) stopping: bool,
}

#[expect(clippy::struct_excessive_bools, reason = "each flag is a state the drawing follows")]
#[derive(Debug, Clone)]
pub(crate) struct ChatView {
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) rows: Vec<ChatRow>,
    pub(crate) empty: bool,
    pub(crate) thinking: Option<Thinking>,
    pub(crate) news: bool,
    pub(crate) busy: bool,
    pub(crate) input: Editor,
    pub(crate) focused: bool,
    pub(crate) placeholder: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChatHit {
    Routines,
    NewConversation,
    Menu,
    Send,
    Stop,
    News,
    Row(RowHit),
    Link(String),
    Input(usize, usize),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChatAreas {
    pub(crate) header: Rect,
    pub(crate) routines: Rect,
    pub(crate) new_conversation: Rect,
    pub(crate) menu: Rect,
    pub(crate) rule: Rect,
    pub(crate) lines: Rect,
    pub(crate) input: Rect,
    pub(crate) field: Rect,
    pub(crate) input_text: Rect,
    pub(crate) send: Rect,
}

pub(crate) fn chat_areas(pane: Rect, input_rows: u16, pitch: u16) -> ChatAreas {
    let margin = MARGIN.min(pane.width / 2);
    let inner = Rect { x: pane.x + margin, width: pane.width.saturating_sub(2 * margin), ..pane };
    let rows = input_rows.clamp(1, MAX_INPUT).max(pitch);
    let header = Rect { height: pitch.min(inner.height), ..inner };
    let rule = Rect { y: header.bottom(), height: 1.min(inner.bottom().saturating_sub(header.bottom())), ..inner };
    let input_y = inner.bottom().saturating_sub(rows).max(rule.bottom());
    let input = Rect { y: input_y, height: inner.bottom().saturating_sub(input_y), ..inner };
    let lines = Rect { y: rule.bottom(), height: input.y.saturating_sub(rule.bottom()), ..inner };
    let send_width = SEND_WIDTH.min(inner.width);
    let send = Rect { x: inner.right() - send_width, width: send_width, height: pitch.min(input.height), ..input };
    let field = Rect { width: inner.width.saturating_sub(send_width + 1), ..input };
    let input_text = Rect { x: field.x + 1, width: field.width.saturating_sub(1), ..field };
    let menu = Rect { x: header.right().saturating_sub(MENU_WIDTH), width: MENU_WIDTH.min(header.width), ..header };
    let new_conversation = Rect {
        x: menu.x.saturating_sub(NEW_CONVERSATION_WIDTH + 1).max(header.x),
        width: NEW_CONVERSATION_WIDTH,
        ..header
    }
    .intersection(header);
    let routines_x = new_conversation.x.saturating_sub(ROUTINES_WIDTH + 1);
    let routines = if routines_x < header.x + NAME_ROOM {
        Rect::default()
    } else {
        Rect { x: routines_x, width: ROUTINES_WIDTH, ..header }.intersection(header)
    };
    ChatAreas { header, routines, new_conversation, menu, rule, lines, input, field, input_text, send }
}

pub(crate) fn input_rows(input: &Editor, pane: Rect) -> u16 {
    u16::try_from(input.lines(usize::from(chat_areas(pane, 1, 1).input_text.width).max(1)).len())
        .unwrap_or(MAX_INPUT)
        .clamp(1, MAX_INPUT)
}

pub(crate) fn body(lines: Rect, thinking: bool) -> Rect {
    Rect { height: lines.height.saturating_sub(u16::from(thinking)), ..lines }
}

pub(crate) fn thinking_row(lines: Rect) -> Rect {
    Rect { y: lines.bottom().saturating_sub(1), height: lines.height.min(1), ..lines }
}

fn thinking_prefix(thinking: &Thinking) -> usize {
    3 + thinking.text.width() + 3
}

pub(crate) fn stop_button(lines: Rect, thinking: &Thinking) -> Rect {
    if thinking.stopping {
        return Rect::default();
    }
    let row = thinking_row(lines);
    let x = row.x.saturating_add(u16::try_from(thinking_prefix(thinking)).unwrap_or(u16::MAX));
    Rect { x, width: STOP_WIDTH, ..row }.intersection(row)
}

pub(crate) fn news_pill(body: Rect) -> Rect {
    let width = NEWS_WIDTH.min(body.width);
    Rect { x: body.right() - width, y: body.bottom().saturating_sub(1), width, height: body.height.min(1) }
}

pub(crate) fn row_y(body: Rect, count: usize, empty: bool, i: usize) -> u16 {
    let count = u16::try_from(count).unwrap_or(u16::MAX).min(body.height);
    let top = if empty { body.y + (body.height - count) / 2 } else { body.bottom() - count };
    top.saturating_add(u16::try_from(i).unwrap_or(u16::MAX))
}

pub(super) fn dim(tone: Tone) -> Style {
    Style::default().fg(tone.muted)
}

fn pad(width: usize, used: usize) -> String {
    " ".repeat(width.saturating_sub(used))
}

fn bubble_width(body: usize) -> usize {
    (body * BUBBLE_SHARE / 100).max(MIN_BUBBLE).min(body).saturating_sub(2).max(1)
}

fn you_rows(text: &str, stamp: &str, width: usize, tone: Tone) -> Vec<ChatRow> {
    let mut lines = wrap_text(text, bubble_width(width));
    if lines.is_empty() {
        lines.push(String::new());
    }
    let room = bubble_width(width);
    let stamp_width = stamp.width();
    let fits = lines.last().is_some_and(|last| last.width() + 1 + stamp_width <= room);
    let mut cells: Vec<(String, String)> = lines.into_iter().map(|line| (line, String::new())).collect();
    if fits {
        if let Some(last) = cells.last_mut() {
            last.1 = stamp.to_string();
        }
    } else {
        cells.push((String::new(), stamp.to_string()));
    }
    let inner = cells.iter().map(|(text, stamp)| text.width() + usize::from(!stamp.is_empty()) + stamp.width());
    let wide = inner.max().unwrap_or(1).min(room).max(1);
    let fill = Style::default().bg(tone.surface);
    cells
        .into_iter()
        .map(|(text, stamp)| {
            let gap = wide.saturating_sub(text.width() + stamp.width());
            let spans = vec![
                Span::raw(pad(width, wide + 2)),
                Span::styled(format!(" {text}{}", " ".repeat(gap)), fill),
                Span::styled(format!("{stamp} "), fill.fg(tone.muted)),
            ];
            ChatRow::plain(Line::from(spans))
        })
        .collect()
}

pub(super) fn bot_rows(head: &str, body: &[Line<'static>], tone: Tone) -> Vec<ChatRow> {
    let mut rows = vec![ChatRow::plain(Line::from(Span::styled(head.to_string(), dim(tone))))];
    rows.extend(body.iter().cloned().map(|line| ChatRow::whole(line, RowHit::Link)));
    rows
}

pub(super) fn step_rows(
    first: u64,
    texts: &[String],
    (open, shown): (bool, usize),
    width: usize,
    tone: Tone,
) -> Vec<ChatRow> {
    let hidden = if open { 0 } else { texts.len().saturating_sub(shown) };
    let mut rows = Vec::new();
    if hidden > 0 {
        let label = format!("+{}", plural(hidden, "step", "steps"));
        rows.push(ChatRow::whole(Line::from(Span::styled(label, dim(tone))), RowHit::Unfold(first)));
    }
    for text in &texts[hidden..] {
        let line = truncate_right(&format!("▸ {text}"), width);
        rows.push(ChatRow::plain(Line::from(Span::styled(line, dim(tone)))));
    }
    rows
}

pub(super) fn error_rows(text: &str, again: Option<(RowHit, &'static str)>, width: usize) -> Vec<ChatRow> {
    let red = Style::default().fg(Color::Red);
    let mut rows: Vec<ChatRow> = wrap_text(text, width.max(1))
        .into_iter()
        .map(|line| ChatRow::plain(Line::from(Span::styled(line, red))))
        .collect();
    if let (Some((hit, label)), Some(last)) = (again, rows.last_mut()) {
        let start = u16::try_from(last.line.width() + 1).unwrap_or(u16::MAX);
        let width = u16::try_from(label.width() + 2).unwrap_or(u16::MAX);
        let shown = format!(" {label} ");
        last.line.spans.extend([Span::raw(" "), Span::styled(shown, Style::default().fg(Color::Cyan))]);
        last.hit = Some((hit, start..start.saturating_add(width)));
    }
    rows
}

pub(super) fn notice_rows(text: &str, width: usize, tone: Tone) -> Vec<ChatRow> {
    let text = truncate_right(&format!("── {text} ──"), width);
    let lead = pad(width, text.width()).len() / 2;
    vec![ChatRow::plain(Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(text, dim(tone))]))]
}

pub(crate) fn entry_rows(entry: &ChatEntry, width: u16, tone: Tone) -> Vec<ChatRow> {
    let width = usize::from(width);
    match entry {
        ChatEntry::You { text, stamp } => you_rows(text, stamp, width, tone),
        ChatEntry::Bot { head, body } => bot_rows(head, body, tone),
        ChatEntry::Steps { first, texts, open } => step_rows(*first, texts, (*open, STEPS_SHOWN), width, tone),
        ChatEntry::Error { id, text, retry } => error_rows(text, retry.then_some((RowHit::Retry(*id), "retry")), width),
        ChatEntry::Notice(text) => notice_rows(text, width, tone),
        ChatEntry::Run(block) => run_rows(block, width, tone),
    }
}

pub(crate) fn empty_rows(width: u16, tone: Tone) -> Vec<ChatRow> {
    let width = usize::from(width);
    let centred = |text: &str, style: Style| {
        let lead = width.saturating_sub(text.width()) / 2;
        Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(text.to_string(), style)])
    };
    let mut rows: Vec<ChatRow> = INTRO
        .iter()
        .flat_map(|text| wrap_text(text, width.max(1)))
        .map(|text| ChatRow::plain(centred(&text, dim(tone))))
        .collect();
    rows.push(ChatRow::blank());
    for (i, example) in EXAMPLES.iter().enumerate() {
        let text = truncate_right(example, width);
        rows.push(ChatRow::whole(centred(&text, Style::default().fg(Color::Cyan)), RowHit::Example(i)));
    }
    rows
}

pub(crate) fn chat_rows(entries: &[ChatEntry], width: u16, tone: Tone) -> Vec<ChatRow> {
    let mut rows = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 && !matches!(entry, ChatEntry::Steps { .. }) {
            rows.push(ChatRow::blank());
        }
        rows.extend(entry_rows(entry, width, tone));
    }
    rows
}

pub(crate) fn link_at(row: &ChatRow, col: usize) -> Option<String> {
    let text: String = row.line.spans.iter().map(|span| span.content.as_ref()).collect();
    let mut start = 0;
    for word in text.split(' ') {
        let end = start + word.width();
        if (start..end).contains(&col) {
            return web_address(word);
        }
        start = end + 1;
    }
    None
}

fn web_address(word: &str) -> Option<String> {
    let at = word.find("http://").or_else(|| word.find("https://"))?;
    let url = word[at..].trim_end_matches([')', '.', ',', ';', ':', '!', '?', '>', ']', '"', '\'']);
    (url.len() > "https://".len() - 1).then(|| url.to_string())
}

pub(crate) fn chat_hit(areas: &ChatAreas, chat: &ChatView, pos: Position) -> Option<ChatHit> {
    let controls = [
        (areas.menu, ChatHit::Menu),
        (areas.routines, ChatHit::Routines),
        (areas.new_conversation, ChatHit::NewConversation),
        (areas.send, ChatHit::Send),
    ];
    if let Some((_, hit)) = controls.into_iter().find(|(r, _)| r.contains(pos)) {
        return Some(hit);
    }
    if let Some(thinking) = &chat.thinking
        && stop_button(areas.lines, thinking).contains(pos)
    {
        return Some(ChatHit::Stop);
    }
    let body = body(areas.lines, chat.thinking.is_some());
    if chat.news && news_pill(body).contains(pos) {
        return Some(ChatHit::News);
    }
    if areas.field.contains(pos) {
        let line = usize::from(pos.y.saturating_sub(areas.input_text.y)) + first_line(&chat.input, areas.input_text);
        return Some(ChatHit::Input(line, usize::from(pos.x.saturating_sub(areas.input_text.x))));
    }
    row_hit(body, chat, pos)
}

fn shown_rows(chat: &ChatView, body: Rect) -> &[ChatRow] {
    &chat.rows[chat.rows.len().saturating_sub(usize::from(body.height))..]
}

fn row_hit(body: Rect, chat: &ChatView, pos: Position) -> Option<ChatHit> {
    if !body.contains(pos) {
        return None;
    }
    let shown = shown_rows(chat, body);
    let top = row_y(body, shown.len(), chat.empty, 0);
    let row = shown.get(usize::from(pos.y.checked_sub(top)?))?;
    let col = pos.x.saturating_sub(body.x);
    let (hit, cols) = row.hit.as_ref()?;
    if !cols.contains(&col) {
        return None;
    }
    match hit {
        RowHit::Link => link_at(row, usize::from(col)).map(ChatHit::Link),
        other => Some(ChatHit::Row(other.clone())),
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, chat: &ChatView, pane: Rect, pitch: u16) {
    let areas = chat_areas(pane, input_rows(&chat.input, pane), pitch);
    draw_header(f, view, chat, &areas);
    draw_rows(f, chat, &areas);
    if let Some(thinking) = &chat.thinking {
        draw_thinking(f, view, thinking, &areas);
    }
    let body = body(areas.lines, chat.thinking.is_some());
    if chat.news && !body.is_empty() {
        let pill = news_pill(body);
        let style = button_style(view, pill, Style::default().fg(Color::Black).bg(Color::Cyan), Color::Cyan);
        draw_button(f, pill, "", NEWS, style);
    }
    let at = EditorField {
        field: areas.field,
        text: areas.input_text,
        focused: chat.focused,
        placeholder: &chat.placeholder,
    };
    draw_editor(f, view, &chat.input, at);
    let style = button_style(view, areas.send, Style::default().fg(Color::Cyan), Color::Cyan);
    draw_button(f, areas.send, "", SEND, style);
}

fn draw_header(f: &mut Frame, view: &View, chat: &ChatView, areas: &ChatAreas) {
    let name_end = if areas.routines.is_empty() { areas.new_conversation.x } else { areas.routines.x };
    let room = usize::from(name_end.saturating_sub(areas.header.x));
    let name = truncate_right(&chat.name, room.saturating_sub(1));
    let detail = truncate_right(&chat.detail, room.saturating_sub(name.width() + 3));
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::raw(" "), Span::styled(name, bold)];
    if !detail.is_empty() {
        spans.extend([Span::raw("  "), Span::styled(detail, Style::default().fg(view.muted))]);
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        middle(Rect { width: name_end.saturating_sub(areas.header.x), ..areas.header }),
    );
    let routines = areas.routines;
    let cyan = Style::default().fg(Color::Cyan);
    draw_button(f, routines, "", &format!("{ROUTINES:^8}"), button_style(view, routines, cyan, Color::Cyan));
    let new = areas.new_conversation;
    let idle = if chat.busy { Style::default().fg(view.muted) } else { Style::default().fg(Color::Cyan) };
    let style = if chat.busy { idle } else { button_style(view, new, idle, Color::Cyan) };
    draw_button(f, new, "", &format!("{NEW_CONVERSATION:^16}"), style);
    let menu = areas.menu;
    draw_button(f, menu, "", "⋯", button_style(view, menu, Style::default().fg(Color::Cyan), Color::Cyan));
    let line = "─".repeat(usize::from(areas.rule.width));
    f.render_widget(Paragraph::new(Span::styled(line, Style::default().fg(view.line()))), areas.rule);
}

fn draw_rows(f: &mut Frame, chat: &ChatView, areas: &ChatAreas) {
    let body = body(areas.lines, chat.thinking.is_some());
    let shown = shown_rows(chat, body);
    for (i, row) in shown.iter().enumerate() {
        let y = row_y(body, shown.len(), chat.empty, i);
        f.render_widget(Paragraph::new(row.line.clone()), Rect { y, height: 1, ..body });
    }
}

fn draw_thinking(f: &mut Frame, view: &View, thinking: &Thinking, areas: &ChatAreas) {
    let row = thinking_row(areas.lines);
    let icon = status_icon(view.muted, Status::Working, view.spin);
    let text = Span::styled(thinking.text.clone(), Style::default().fg(view.muted));
    f.render_widget(Paragraph::new(Line::from(vec![Span::raw(" "), icon, Span::raw(" "), text])), row);
    let stop = stop_button(areas.lines, thinking);
    if !stop.is_empty() {
        draw_button(f, stop, "", "stop", button_style(view, stop, Style::default().fg(Color::Cyan), Color::Cyan));
    }
}
