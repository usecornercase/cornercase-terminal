use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier, Style};

use super::stack::{Look, Slot, cells, count};
use super::{Hit, dim};
use crate::editor::wrap;
use crate::text;
use crate::ui::theme::WAITING_COLOR;

const TITLE: &str = "Messages";
const EMPTY: &str = "nothing yet";
const LEAD: u16 = 2;
const PREVIEW: usize = 2;
const GUTTER: &str = "▎";
const INDENT: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    Forward(usize),
    Loop(String),
    Outside,
}

impl Step {
    fn label(&self) -> String {
        match self {
            Self::Forward(stage) => format!("stage {} → {}", stage + 1, stage + 2),
            Self::Loop(when) => when.clone(),
            Self::Outside => "outside the plan".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) seq: u64,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: &'static str,
    pub(crate) step: Option<Step>,
    pub(crate) colour: Color,
    pub(crate) age: String,
    pub(crate) head: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct History {
    pub(crate) entries: Vec<Entry>,
    pub(crate) recorded: u64,
}

fn preview(head: Option<&str>, width: u16) -> Vec<String> {
    let width = usize::from(width.saturating_sub(INDENT).max(1));
    let mut rows = Vec::new();
    for line in head.unwrap_or_default().lines() {
        let chars: Vec<char> = line.chars().collect();
        rows.extend(wrap(line, width).into_iter().map(|range| chars[range].iter().collect::<String>()));
        if rows.len() > PREVIEW {
            break;
        }
    }
    let more = rows.len() > PREVIEW;
    rows.truncate(PREVIEW);
    if more && let Some(last) = rows.last_mut() {
        *last = text::truncate_cells(&format!("{last}…"), width);
    }
    rows
}

struct Plan {
    inline: bool,
    lines: Vec<String>,
}

impl Plan {
    fn of(entry: &Entry, width: u16) -> Self {
        let step = entry.step.as_ref().map_or(0, |step| cells(&format!(" · {}", step.label())));
        let parts = [INDENT, cells(&entry.from), cells(" → "), cells(&entry.to), cells(&format!(" · {}", entry.kind))];
        let left = parts.into_iter().fold(0u16, u16::saturating_add);
        let tail = cells(&format!("#{} · {}", entry.seq, entry.age));
        let inline = left.saturating_add(step).saturating_add(tail.saturating_add(1)) <= width;
        Self { inline, lines: preview(entry.head.as_deref(), width) }
    }

    fn rows(&self, entry: &Entry) -> u16 {
        1u16.saturating_add(u16::from(!self.inline && entry.step.is_some())).saturating_add(count(self.lines.len()))
    }
}

pub(super) fn height(history: &History, width: u16) -> u16 {
    let rows = history.entries.iter().fold(0u16, |n, entry| n.saturating_add(Plan::of(entry, width).rows(entry)));
    LEAD.saturating_add(if history.entries.is_empty() { 1 } else { rows })
}

pub(super) fn hit(slot: &Slot, history: &History, pos: Position) -> Option<Hit> {
    let row = slot.row_at(pos)?.checked_sub(LEAD)?;
    let mut top = 0u16;
    for entry in &history.entries {
        let rows = Plan::of(entry, slot.area.width).rows(entry);
        if row < top.saturating_add(rows) {
            return Some(Hit::Message(entry.seq));
        }
        top = top.saturating_add(rows);
    }
    None
}

fn step_style(step: &Step, look: Look) -> Style {
    if step == &Step::Outside { Style::default().fg(WAITING_COLOR) } else { dim(look.muted) }
}

fn first_row(buf: &mut Buffer, slot: &Slot, (row, entry): (u16, &Entry), (inline, look): (bool, Look)) {
    let tail = slot.put_right(buf, row, &format!("#{} · {}", entry.seq, entry.age), dim(look.muted));
    let limit = slot.area.width.saturating_sub(tail.saturating_add(1));
    let mut x = INDENT;
    let mut put = |buf: &mut Buffer, text: &str, style: Style| {
        let room = usize::from(limit.saturating_sub(x));
        if room >= 2 {
            slot.put(buf, (row, x), &text::truncate_cells(text, room), style);
        }
        x = x.saturating_add(cells(text));
    };
    put(buf, &entry.from, Style::default());
    put(buf, " → ", dim(look.muted));
    put(buf, &entry.to, Style::default().fg(entry.colour).add_modifier(Modifier::BOLD));
    put(buf, &format!(" · {}", entry.kind), dim(look.muted));
    if let Some(step) = entry.step.as_ref().filter(|_| inline) {
        put(buf, &format!(" · {}", step.label()), step_style(step, look));
    }
}

fn draw_entry(buf: &mut Buffer, slot: &Slot, (top, entry): (u16, &Entry), look: Look) -> u16 {
    let plan = Plan::of(entry, slot.area.width);
    let rows = plan.rows(entry);
    let end = top.saturating_add(rows);
    slot.shade(buf, top..end, look);
    for row in top..end {
        slot.put(buf, (row, 0), GUTTER, Style::default().fg(entry.colour));
    }
    first_row(buf, slot, (top, entry), (plan.inline, look));
    let mut next = top.saturating_add(1);
    if let Some(step) = entry.step.as_ref().filter(|_| !plan.inline) {
        slot.put(buf, (next, INDENT), &step.label(), step_style(step, look));
        next = next.saturating_add(1);
    }
    for (i, line) in plan.lines.iter().enumerate() {
        slot.put(buf, (next.saturating_add(count(i)), INDENT), line, dim(look.muted));
    }
    rows
}

pub(super) fn draw(buf: &mut Buffer, slot: &Slot, history: &History, look: Look) {
    slot.title(buf, 1, (TITLE, &history.recorded.to_string()), look.muted);
    if history.entries.is_empty() {
        slot.put(buf, (LEAD, 0), EMPTY, dim(look.muted));
        return;
    }
    let mut top = LEAD;
    for entry in &history.entries {
        top = top.saturating_add(draw_entry(buf, slot, (top, entry), look));
    }
}
