use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::chat::{ChatRow, RowHit, Tone, dim};
use crate::agents::activity::Status;
use crate::text::{plural, truncate_right};
use crate::ui::theme::status_icon;

const BODY_SHOWN: usize = 6;
const INDENT: &str = "  ";
const SHOW: &str = " show ";
const CLOSED: &str = "closed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChatAgentKind {
    Started,
    Finished { failed: bool },
    Waiting,
}

#[derive(Debug, Clone)]
pub(crate) struct ChatAgentLine {
    pub(crate) id: u64,
    pub(crate) state: ChatAgentKind,
    pub(crate) text: String,
    pub(crate) body: Vec<Line<'static>>,
    pub(crate) open: bool,
    pub(crate) link: Option<u64>,
}

fn icon_style(kind: ChatAgentKind, tone: Tone) -> Style {
    match kind {
        ChatAgentKind::Started => dim(tone),
        ChatAgentKind::Finished { failed: false } => status_icon(tone.muted, Status::Done, None).style,
        ChatAgentKind::Finished { failed: true } => status_icon(tone.muted, Status::Failed, None).style,
        ChatAgentKind::Waiting => status_icon(tone.muted, Status::Waiting, None).style,
    }
}

fn head_row(line: &ChatAgentLine, width: usize, tone: Tone) -> ChatRow {
    let tail = if line.link.is_some() { SHOW } else { CLOSED };
    let room = width.saturating_sub(tail.width() + 1);
    let text = truncate_right(&line.text, room);
    let mut chars = text.chars();
    let icon = chars.next().map(String::from).unwrap_or_default();
    let rest = chars.as_str().to_string();
    let rest_style = if line.state == ChatAgentKind::Started { dim(tone) } else { Style::default() };
    let start = u16::try_from(text.width() + 1).unwrap_or(u16::MAX);
    let mut spans =
        vec![Span::styled(icon, icon_style(line.state, tone)), Span::styled(rest, rest_style), Span::raw(" ")];
    let mut row = ChatRow::plain(Line::default());
    if let Some(pane) = line.link {
        spans.push(Span::styled(SHOW, Style::default().fg(Color::Cyan)));
        let end = start.saturating_add(u16::try_from(SHOW.width()).unwrap_or(u16::MAX));
        row.hit = Some((RowHit::Show(pane), start..end));
    } else {
        spans.push(Span::styled(CLOSED, dim(tone)));
    }
    row.line = Line::from(spans);
    row
}

fn body_rows(line: &ChatAgentLine, tone: Tone) -> Vec<ChatRow> {
    let hidden = if line.open { 0 } else { line.body.len().saturating_sub(BODY_SHOWN) };
    let dimmed = Style::default().add_modifier(Modifier::DIM);
    let mut rows: Vec<ChatRow> = line
        .body
        .iter()
        .take(line.body.len() - hidden)
        .map(|row| {
            let mut spans = vec![Span::raw(INDENT)];
            spans.extend(row.spans.iter().cloned());
            ChatRow::plain(Line::from(spans).patch_style(dimmed))
        })
        .collect();
    if hidden > 0 {
        let label = format!("{INDENT}+{}", plural(hidden, "line", "lines"));
        let end = u16::try_from(label.width()).unwrap_or(u16::MAX);
        let mut row = ChatRow::plain(Line::from(Span::styled(label, dim(tone))));
        row.hit = Some((RowHit::Unfold(line.id), 0..end));
        rows.push(row);
    }
    rows
}

pub(super) fn chat_agent_rows(line: &ChatAgentLine, width: usize, tone: Tone) -> Vec<ChatRow> {
    let mut rows = vec![head_row(line, width, tone)];
    rows.extend(body_rows(line, tone));
    rows
}
