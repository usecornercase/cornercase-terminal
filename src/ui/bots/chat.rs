use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::agent::{ChatAgentLine, chat_agent_rows};
use super::run::{RunBlock, run_rows};
use crate::agents::activity::Status;
use crate::editor::{Editor, wrap_text};
use crate::text::{plural, truncate_right};
use crate::ui::View;
use crate::ui::theme::status_icon;
use crate::ui::widgets::{EditorField, button_style, draw_button, draw_editor, first_line, middle, sidebar_hovered};

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
const MARGIN: u16 = 2;
const COLUMN: u16 = 110;
const AIRY_FROM: u16 = 14;
const PAD: u16 = 2;
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
const BUBBLE_PAD: usize = 2;
const TOP_HALF: &str = "▄";
const BOTTOM_HALF: &str = "▀";
const EXAMPLE_MARK: &str = " › ";

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
    Show(u64),
    Example(usize),
    Give(u64),
    Link(u64),
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
    Bot { id: u64, head: String, body: Vec<Line<'static>> },
    Steps { first: u64, texts: Vec<String>, open: bool },
    Error { id: u64, text: String, retry: bool },
    Notice(String),
    Asking { text: String, ask: u64 },
    Run(RunBlock),
    Agent(ChatAgentLine),
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
    pub(crate) selected: Vec<(usize, Range<u16>)>,
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
    Link(u64, String),
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

fn column(pane: Rect) -> Rect {
    let margin = MARGIN.min(pane.width / 4);
    let room = pane.width.saturating_sub(2 * margin);
    let width = room.min(COLUMN);
    Rect { x: pane.x + margin + (room - width) / 2, width, ..pane }
}

fn band_of(input: Rect) -> Rect {
    if input.height >= 3 { Rect { y: input.y + 1, height: input.height - 2, ..input } } else { input }
}

fn composer(inner: Rect, top: u16, rows: u16, pitch: u16) -> (Rect, Rect, Rect) {
    let height = (rows + 2).max(pitch);
    let y = inner.bottom().saturating_sub(height).max(top);
    let input = Rect { y, height: inner.bottom().saturating_sub(y), ..inner };
    let send_width = SEND_WIDTH.min(input.width);
    let send =
        Rect { x: input.right().saturating_sub(send_width + PAD), width: send_width, ..input }.intersection(input);
    let band = band_of(input);
    let input_text = Rect {
        x: input.x + PAD.min(input.width),
        y: band.y,
        width: input.width.saturating_sub(2 * PAD + send_width + 1),
        height: rows.min(band.height),
    };
    (input, input_text, send)
}

fn header_buttons(header: Rect) -> (Rect, Rect, Rect) {
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
    (routines, new_conversation, menu)
}

pub(crate) fn chat_areas(pane: Rect, input_rows: u16, pitch: u16) -> ChatAreas {
    let inner = column(pane);
    let rows = input_rows.clamp(1, MAX_INPUT);
    let header = Rect { height: pitch.min(inner.height), ..inner };
    let rule = Rect { y: header.bottom(), height: 1.min(inner.bottom().saturating_sub(header.bottom())), ..inner };
    let (input, input_text, send) = composer(inner, rule.bottom(), rows, pitch);
    let air = u16::from(inner.height >= AIRY_FROM);
    let top = (rule.bottom() + air).min(input.y);
    let lines = Rect { y: top, height: input.y.saturating_sub(air).saturating_sub(top), ..inner };
    let (routines, new_conversation, menu) = header_buttons(header);
    ChatAreas { header, routines, new_conversation, menu, rule, lines, input, field: input, input_text, send }
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
    2 + thinking.text.width() + 3
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
    (body * BUBBLE_SHARE / 100).max(MIN_BUBBLE).min(body).saturating_sub(2 * BUBBLE_PAD).max(1)
}

fn bubble_cells(text: &str, stamp: &str, room: usize) -> Vec<(String, String)> {
    let mut lines = wrap_text(text, room);
    if lines.is_empty() {
        lines.push(String::new());
    }
    let fits = lines.last().is_some_and(|last| last.width() + 1 + stamp.width() <= room);
    let mut cells: Vec<(String, String)> = lines.into_iter().map(|line| (line, String::new())).collect();
    match cells.last_mut() {
        Some(last) if fits => last.1 = stamp.to_string(),
        _ => cells.push((String::new(), stamp.to_string())),
    }
    cells
}

fn you_rows(text: &str, stamp: &str, width: usize, tone: Tone) -> Vec<ChatRow> {
    let room = bubble_width(width);
    let cells = bubble_cells(text, stamp, room);
    let inner = cells.iter().map(|(text, stamp)| text.width() + usize::from(!stamp.is_empty()) + stamp.width());
    let wide = inner.max().unwrap_or(1).min(room).max(1);
    let outer = wide + 2 * BUBBLE_PAD;
    let (fill, side) = (Style::default().bg(tone.surface), " ".repeat(BUBBLE_PAD));
    let edge = |glyph: &str| {
        let spans =
            vec![Span::raw(pad(width, outer)), Span::styled(glyph.repeat(outer), Style::default().fg(tone.surface))];
        ChatRow::plain(Line::from(spans))
    };
    let mut rows = vec![edge(TOP_HALF)];
    rows.extend(cells.into_iter().map(|(text, stamp)| {
        let gap = wide.saturating_sub(text.width() + stamp.width());
        ChatRow::plain(Line::from(vec![
            Span::raw(pad(width, outer)),
            Span::styled(format!("{side}{text}{}", " ".repeat(gap)), fill),
            Span::styled(format!("{stamp}{side}"), fill.fg(tone.muted)),
        ]))
    }));
    rows.push(edge(BOTTOM_HALF));
    rows
}

fn head_line(head: &str, tone: Tone) -> Line<'static> {
    let (name, when) = head.rsplit_once(" · ").unwrap_or((head, ""));
    let mut spans = vec![Span::styled(name.to_string(), Style::default().add_modifier(Modifier::BOLD))];
    if !when.is_empty() {
        spans.push(Span::styled(format!(" · {when}"), dim(tone)));
    }
    Line::from(spans)
}

pub(super) fn bot_rows(id: u64, head: &str, body: &[Line<'static>], tone: Tone) -> Vec<ChatRow> {
    let mut rows = vec![ChatRow::plain(head_line(head, tone))];
    rows.extend(body.iter().cloned().map(|line| ChatRow::whole(line, RowHit::Link(id))));
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

pub(super) fn error_rows(text: &str, again: Option<(RowHit, &'static str)>, width: usize, tone: Tone) -> Vec<ChatRow> {
    let red = Style::default().fg(Color::Red);
    let mut rows: Vec<ChatRow> = wrap_text(text, width.max(1))
        .into_iter()
        .map(|line| ChatRow::plain(Line::from(Span::styled(line, red))))
        .collect();
    if let Some((hit, label)) = again {
        with_button(&mut rows, hit, label, tone);
    }
    rows
}

pub(super) fn pill(tone: Tone) -> Style {
    Style::default().fg(Color::Cyan).bg(tone.surface)
}

fn with_button(rows: &mut [ChatRow], hit: RowHit, label: &str, tone: Tone) {
    if let Some(last) = rows.last_mut() {
        let start = u16::try_from(last.line.width() + 2).unwrap_or(u16::MAX);
        let width = u16::try_from(label.width() + 2).unwrap_or(u16::MAX);
        let shown = format!(" {label} ");
        last.line.spans.extend([Span::raw("  "), Span::styled(shown, pill(tone))]);
        last.hit = Some((hit, start..start.saturating_add(width)));
    }
}

pub(super) fn notice_rows(text: &str, width: usize, tone: Tone) -> Vec<ChatRow> {
    wrap_text(text, width.saturating_sub(6).max(1))
        .into_iter()
        .map(|row| {
            let row = truncate_right(&format!("── {} ──", row.trim_end()), width);
            let lead = pad(width, row.width()).len() / 2;
            ChatRow::plain(Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(row, dim(tone))]))
        })
        .collect()
}

fn asking_rows(text: &str, ask: u64, width: usize, tone: Tone) -> Vec<ChatRow> {
    const GIVE: &str = "give…";
    let mut rows = notice_rows(text, width.saturating_sub(GIVE.width() + 4), tone);
    with_button(&mut rows, RowHit::Give(ask), GIVE, tone);
    rows
}

pub(crate) fn entry_rows(entry: &ChatEntry, width: u16, tone: Tone) -> Vec<ChatRow> {
    let width = usize::from(width);
    match entry {
        ChatEntry::You { text, stamp } => you_rows(text, stamp, width, tone),
        ChatEntry::Bot { id, head, body } => bot_rows(*id, head, body, tone),
        ChatEntry::Steps { first, texts, open } => step_rows(*first, texts, (*open, STEPS_SHOWN), width, tone),
        ChatEntry::Error { id, text, retry } => {
            error_rows(text, retry.then_some((RowHit::Retry(*id), "retry")), width, tone)
        }
        ChatEntry::Notice(text) => notice_rows(text, width, tone),
        ChatEntry::Asking { text, ask } => asking_rows(text, *ask, width, tone),
        ChatEntry::Run(block) => run_rows(block, width, tone),
        ChatEntry::Agent(line) => chat_agent_rows(line, width, tone),
    }
}

fn example_row(i: usize, (lead, wide): (usize, usize), tone: Tone) -> ChatRow {
    let fill = Style::default().bg(tone.surface);
    let room = wide.saturating_sub(EXAMPLE_MARK.width() + 1);
    let text = truncate_right(EXAMPLES[i], room);
    let spans = vec![
        Span::raw(" ".repeat(lead)),
        Span::styled(EXAMPLE_MARK, fill.fg(Color::Cyan)),
        Span::styled(format!("{text}{}", pad(wide.saturating_sub(EXAMPLE_MARK.width()), text.width())), fill),
    ];
    let start = u16::try_from(lead).unwrap_or(u16::MAX);
    let end = start.saturating_add(u16::try_from(wide).unwrap_or(u16::MAX));
    ChatRow { line: Line::from(spans), hit: Some((RowHit::Example(i), start..end)) }
}

pub(crate) fn empty_rows(width: u16, height: u16, tone: Tone) -> Vec<ChatRow> {
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
    let longest = EXAMPLES.iter().map(|example| example.width()).max().unwrap_or_default();
    let wide = (longest + EXAMPLE_MARK.width() + 2).min(width);
    let lead = (width - wide) / 2;
    let spaced = rows.len() + 2 * EXAMPLES.len() <= usize::from(height);
    for i in 0..EXAMPLES.len() {
        if i == 0 || spaced {
            rows.push(ChatRow::blank());
        }
        rows.push(example_row(i, (lead, wide), tone));
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

pub(crate) fn whole_address<'a>(drawn: &str, sources: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut longer = None;
    for address in sources.into_iter().flat_map(str::split_whitespace).filter_map(web_address) {
        if address == drawn {
            return Some(address);
        }
        if longer.is_none() && address.starts_with(drawn) {
            longer = Some(address);
        }
    }
    longer
}

const PUNCTUATION: [char; 11] = [')', '.', ',', ';', ':', '!', '?', '>', ']', '"', '\''];

fn web_address(word: &str) -> Option<String> {
    let at = word.find("http://").or_else(|| word.find("https://"))?;
    let closing: String = word[..at].chars().rev().take_while(|c| matches!(c, '*' | '_' | '~')).collect();
    let url = word[at..].trim_end_matches(PUNCTUATION);
    let url = url.strip_suffix(closing.as_str()).map_or(url, |url| url.trim_end_matches(PUNCTUATION));
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

pub(crate) fn chat_cell(areas: &ChatAreas, chat: &ChatView, pos: Position) -> Option<(usize, u16)> {
    let body = body(areas.lines, chat.thinking.is_some());
    let shown = shown_rows(chat, body);
    let last = shown.len().checked_sub(1).filter(|_| !chat.empty && !body.is_empty())?;
    let top = row_y(body, shown.len(), false, 0);
    let end = body.width.saturating_sub(1);
    if pos.y < top {
        return Some((0, 0));
    }
    let row = usize::from(pos.y - top);
    if row > last {
        return Some((last, end));
    }
    Some((row, pos.x.saturating_sub(body.x).min(end)))
}

pub(crate) fn row_cells(row: &ChatRow, cols: &Range<u16>) -> String {
    let mut at = 0;
    let mut kept = String::new();
    for c in row.line.spans.iter().flat_map(|span| span.content.chars()) {
        if (cols.start..cols.end).contains(&at) {
            kept.push(c);
        }
        at = at.saturating_add(u16::try_from(c.to_string().width()).unwrap_or(1));
    }
    kept
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
        RowHit::Link(line) => link_at(row, usize::from(col)).map(|url| ChatHit::Link(*line, url)),
        other => Some(ChatHit::Row(other.clone())),
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, chat: &ChatView, pane: Rect, pitch: u16) {
    let areas = chat_areas(pane, input_rows(&chat.input, pane), pitch);
    draw_header(f, view, chat, &areas);
    draw_rows(f, view, chat, &areas);
    if let Some(thinking) = &chat.thinking {
        draw_thinking(f, view, thinking, &areas);
    }
    let body = body(areas.lines, chat.thinking.is_some());
    if chat.news && !body.is_empty() {
        let pill = news_pill(body);
        let style = button_style(view, pill, Style::default().fg(Color::Black).bg(Color::Cyan), Color::Cyan);
        draw_button(f, pill, "", NEWS, style);
    }
    draw_composer(f, view, chat, &areas);
}

fn draw_composer(f: &mut Frame, view: &View, chat: &ChatView, areas: &ChatAreas) {
    let input = areas.input;
    let band = band_of(input);
    if band != input {
        let edge = Style::default().fg(view.surface());
        for (y, glyph) in [(input.y, TOP_HALF), (input.bottom() - 1, BOTTOM_HALF)] {
            let line = Span::styled(glyph.repeat(usize::from(input.width)), edge);
            f.render_widget(Paragraph::new(line), Rect { y, height: 1, ..input });
        }
    }
    let at = EditorField {
        field: band,
        text: areas.input_text,
        focused: chat.focused,
        placeholder: &chat.placeholder,
        top: None,
    };
    draw_editor(f, view, &chat.input, at);
    let idle = if chat.input.is_empty() {
        Style::default().fg(view.muted).bg(view.surface())
    } else {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    };
    let send = Rect { y: areas.input_text.bottom().saturating_sub(1).max(band.y), height: 1, ..areas.send };
    draw_button(f, send.intersection(band), "", SEND, button_style(view, areas.send, idle, Color::Cyan));
}

fn header_pill(f: &mut Frame, view: &View, r: Rect, (label, busy): (&str, bool)) {
    let fill = Style::default().bg(view.surface());
    let style = if busy { fill.fg(view.muted) } else { button_style(view, r, fill.fg(Color::Cyan), Color::Cyan) };
    draw_button(f, r, "", label, style);
}

fn draw_header(f: &mut Frame, view: &View, chat: &ChatView, areas: &ChatAreas) {
    let name_end = if areas.routines.is_empty() { areas.new_conversation.x } else { areas.routines.x };
    let room = usize::from(name_end.saturating_sub(areas.header.x));
    let name = truncate_right(&chat.name, room.saturating_sub(1));
    let detail = truncate_right(&chat.detail, room.saturating_sub(name.width() + 4));
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut spans = vec![Span::styled(name, bold)];
    if !detail.is_empty() {
        spans.extend([Span::raw("   "), Span::styled(detail, Style::default().fg(view.muted))]);
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)),
        middle(Rect { width: name_end.saturating_sub(areas.header.x), ..areas.header }),
    );
    header_pill(f, view, areas.routines, (&format!("{ROUTINES:^8}"), false));
    header_pill(f, view, areas.new_conversation, (&format!("{NEW_CONVERSATION:^16}"), chat.busy));
    header_pill(f, view, areas.menu, ("⋯", false));
    let line = "─".repeat(usize::from(areas.rule.width));
    f.render_widget(Paragraph::new(Span::styled(line, Style::default().fg(view.line()))), areas.rule);
}

fn lit_cells(row: &ChatRow, r: Rect) -> Option<Rect> {
    let (hit, cols) = row.hit.as_ref()?;
    if matches!(hit, RowHit::Link(_)) {
        return None;
    }
    let (start, end) = if cols.end == u16::MAX {
        let text: String = row.line.spans.iter().map(|span| span.content.as_ref()).collect();
        let lead = text.width() - text.trim_start().width();
        (u16::try_from(lead).unwrap_or(u16::MAX), u16::try_from(text.trim_end().width()).unwrap_or(u16::MAX))
    } else {
        (cols.start, cols.end)
    };
    (start < end).then(|| Rect { x: r.x.saturating_add(start), width: end - start, ..r }.intersection(r))
}

fn draw_rows(f: &mut Frame, view: &View, chat: &ChatView, areas: &ChatAreas) {
    let body = body(areas.lines, chat.thinking.is_some());
    let shown = shown_rows(chat, body);
    let lit = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    for (i, row) in shown.iter().enumerate() {
        let r = Rect { y: row_y(body, shown.len(), chat.empty, i), height: 1, ..body };
        f.render_widget(Paragraph::new(row.line.clone()), r);
        if let Some(cells) = lit_cells(row, r).filter(|cells| sidebar_hovered(view, *cells)) {
            f.buffer_mut().set_style(cells, lit);
        }
    }
    for (i, cols) in chat.selected.iter().filter(|(i, _)| *i < shown.len()) {
        let r = Rect { y: row_y(body, shown.len(), chat.empty, *i), height: 1, ..body };
        let start = cols.start.min(r.width);
        let cells = Rect { x: r.x + start, width: cols.end.min(r.width).saturating_sub(start), ..r };
        f.buffer_mut().set_style(cells, Style::default().add_modifier(Modifier::REVERSED));
    }
}

fn draw_thinking(f: &mut Frame, view: &View, thinking: &Thinking, areas: &ChatAreas) {
    let row = thinking_row(areas.lines);
    let icon = status_icon(view.muted, Status::Working, view.spin);
    let text = Span::styled(thinking.text.clone(), Style::default().fg(view.muted));
    f.render_widget(Paragraph::new(Line::from(vec![icon, Span::raw(" "), text])), row);
    let stop = stop_button(areas.lines, thinking);
    if !stop.is_empty() {
        let idle = Style::default().fg(Color::Cyan).bg(view.surface());
        draw_button(f, stop, "", "stop", button_style(view, stop, idle, Color::Cyan));
    }
}
