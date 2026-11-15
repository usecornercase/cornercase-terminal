use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

use super::chat::{ChatRow, RowHit, Tone, bot_rows, dim, error_rows, notice_rows, step_rows};
use crate::text::truncate_right;

#[derive(Debug, Clone)]
pub(crate) struct RunBlock {
    pub(crate) run: u64,
    pub(crate) head: String,
    pub(crate) first: u64,
    pub(crate) steps: Vec<String>,
    pub(crate) open: bool,
    pub(crate) answer: Option<(String, Vec<Line<'static>>)>,
    pub(crate) error: Option<String>,
    pub(crate) again: bool,
    pub(crate) end: Option<String>,
    pub(crate) answering: bool,
}

pub(super) fn run_rows(block: &RunBlock, width: usize, tone: Tone) -> Vec<ChatRow> {
    let head = truncate_right(&block.head, width);
    let bold = dim(tone).add_modifier(Modifier::BOLD);
    let mut rows = vec![ChatRow::plain(Line::from(Span::styled(head, bold)))];
    rows.extend(step_rows(block.first, &block.steps, (block.open, 0), width, tone));
    if let Some((head, body)) = &block.answer {
        rows.extend(bot_rows(head, body, tone));
    }
    if let Some(error) = &block.error {
        let again = block.again.then_some((RowHit::RunAgain(block.run), "run again"));
        rows.extend(error_rows(error, again, width));
    }
    if let Some(end) = &block.end {
        rows.extend(notice_rows(end, width, tone));
    }
    if block.answering {
        rows.push(ChatRow::plain(Line::from(Span::styled("answering…", dim(tone)))));
    }
    rows
}
