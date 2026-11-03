use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::text::{cells, truncate_cells, truncate_left};

const PALETTE: [u8; 9] = [141, 175, 110, 180, 139, 73, 174, 146, 104];
const USED: char = '■';
const FREE: char = '□';
const BUFFER: char = '▒';
const GAP: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fill {
    Used(usize),
    Buffer,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Slice {
    pub(crate) label: String,
    pub(crate) tokens: u64,
    pub(crate) text: String,
    pub(crate) share: String,
    pub(crate) fill: Fill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) left: String,
    pub(crate) middle: String,
    pub(crate) right: String,
    pub(crate) muted: bool,
    pub(crate) path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) title: &'static str,
    pub(crate) rows: Vec<Row>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ContextView {
    pub(crate) max: u64,
    pub(crate) slices: Vec<Slice>,
    pub(crate) not_loaded: Option<String>,
    pub(crate) sections: Vec<Section>,
    pub(crate) notice: Option<String>,
}

fn color(index: usize) -> Color {
    Color::Indexed(PALETTE[index % PALETTE.len()])
}

fn boundary(tokens: u64, max: u64, width: usize) -> usize {
    let max = max.max(1);
    let scaled = u128::from(tokens.min(max)) * width as u128;
    usize::try_from((scaled * 2 + u128::from(max)) / (u128::from(max) * 2)).unwrap_or(width).min(width)
}

fn bar(view: &ContextView, width: usize, muted: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let (mut sum, mut at) = (0_u64, 0_usize);
    for slice in view.slices.iter().filter(|s| matches!(s.fill, Fill::Used(_))) {
        sum += slice.tokens;
        let end = boundary(sum, view.max, width);
        if let Fill::Used(index) = slice.fill {
            spans.push(Span::styled(USED.to_string().repeat(end - at), Style::default().fg(color(index))));
        }
        at = end;
    }
    let buffer: u64 = view.slices.iter().filter(|s| s.fill == Fill::Buffer).map(|s| s.tokens).sum();
    let free_end = width - boundary(buffer, view.max, width).min(width - at);
    spans.push(Span::styled(FREE.to_string().repeat(free_end - at), muted));
    spans.push(Span::styled(BUFFER.to_string().repeat(width - free_end), muted));
    Line::from(spans)
}

fn slice_line(slice: &Slice, width: usize, muted: Style) -> Line<'static> {
    let (mark, style) = match slice.fill {
        Fill::Used(index) => (USED, Style::default().fg(color(index))),
        Fill::Buffer => (BUFFER, muted),
        Fill::Free => (FREE, muted),
    };
    let right = format!("{:>7} {:>6}", slice.text, slice.share);
    let name = truncate_cells(&slice.label, width.saturating_sub(cells(&right) + GAP + 2));
    let pad = width.saturating_sub(2 + cells(&name) + cells(&right));
    Line::from(vec![
        Span::styled(format!("{mark} "), style),
        Span::raw(name),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, muted),
    ])
}

fn row_line(row: &Row, width: usize, muted: Style) -> Line<'static> {
    let style = if row.muted { muted } else { Style::default() };
    let middle = if row.middle.is_empty() { String::new() } else { format!("  {}", row.middle) };
    let fixed = cells(&middle) + cells(&row.right) + GAP;
    let room = width.saturating_sub(fixed + 2);
    let left = if row.path { truncate_left(&row.left, room) } else { truncate_cells(&row.left, room) };
    let pad = width.saturating_sub(2 + cells(&left) + cells(&middle) + cells(&row.right));
    Line::from(vec![
        Span::styled(format!("  {left}"), style),
        Span::styled(middle, muted),
        Span::raw(" ".repeat(pad)),
        Span::styled(row.right.clone(), style),
    ])
}

pub(crate) fn context_notes(view: &ContextView, width: usize, muted: Color) -> Vec<Line<'static>> {
    let muted = Style::default().fg(muted);
    let mut lines = Vec::new();
    if !view.slices.is_empty() && width > 0 {
        lines.push(bar(view, width, muted));
        lines.push(Line::default());
        lines.extend(view.slices.iter().map(|slice| slice_line(slice, width, muted)));
    }
    if let Some(text) = &view.not_loaded {
        lines.push(Line::from(Span::styled(truncate_cells(text, width), muted)));
    }
    for section in &view.sections {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            section.title,
            Style::default().add_modifier(ratatui::style::Modifier::BOLD),
        )));
        lines.extend(section.rows.iter().map(|row| row_line(row, width, muted)));
    }
    if let Some(notice) = &view.notice {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.push(Line::from(Span::styled(truncate_cells(notice, width), muted)));
    }
    lines
}

#[cfg(test)]
mod tests;
