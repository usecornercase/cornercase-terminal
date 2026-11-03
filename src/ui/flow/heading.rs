use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use super::stack::{Look, Slot, cells};
use super::{Hit, View, dim};
use crate::flows::ORCHESTRATOR;
use crate::text;
use crate::ui::put;
use crate::ui::theme::{BRAND_COLOR, WAITING_COLOR, line_colour};
use crate::ui::widgets::action_style;

const RUNNING: &str = "running";
const REACHED: &str = "reached its end";
const LEAD_BUTTON: &str = " orchestrator ";
const STOP_BUTTON: &str = " stop ";
const NOT_STARTED: &str = "not started";
const MIN_NAME: usize = 4;
const CLOSE_ROOM: u16 = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Heading {
    pub(crate) place: String,
    pub(crate) run: u64,
    pub(crate) messages: u64,
    pub(crate) active: Option<usize>,
    pub(crate) lead: bool,
}

fn reached(view: &View) -> bool {
    view.diagram.live.as_ref().is_some_and(|live| live.end.reached)
}

fn state(view: &View) -> (&'static str, Color) {
    if reached(view) { (REACHED, BRAND_COLOR) } else { (RUNNING, Color::Yellow) }
}

pub(super) fn draw_title(buf: &mut Buffer, row: Rect, view: &View) {
    let (word, colour) = state(view);
    let room = usize::from(row.width.saturating_sub(CLOSE_ROOM));
    let tail = text::cells(word).saturating_add(3);
    let limit = row.right();
    if room < tail.saturating_add(MIN_NAME) {
        put(buf, (row.x, row.y), &text::truncate_cells(word, room), Style::default().fg(colour), limit);
        return;
    }
    let name = text::truncate_cells(&view.name, room - tail);
    put(buf, (row.x, row.y), &name, Style::default().add_modifier(Modifier::BOLD), limit);
    let x = row.x.saturating_add(u16::try_from(text::cells(&name)).unwrap_or(0));
    put(buf, (x, row.y), " · ", dim(view.diagram.muted), limit);
    put(buf, (x.saturating_add(3), row.y), word, Style::default().fg(colour), limit);
}

fn summary(view: &View) -> Vec<Span<'static>> {
    let stages = view.diagram.stages.len();
    let muted = dim(view.diagram.muted);
    let first = match view.heading.active {
        Some(active) => format!("stage {} of {stages}", active + 1),
        None => NOT_STARTED.into(),
    };
    let mut spans = vec![Span::styled(first, Style::default())];
    for back in view.diagram.loops.iter().flatten().filter(|back| back.rounds > 0) {
        let style = if back.rounds > back.at_most { Style::default().fg(WAITING_COLOR) } else { muted };
        spans.push(Span::styled(format!(" · {} {}/{}", back.when, back.rounds, back.at_most), style));
    }
    spans
}

fn buttons_width(view: &View) -> u16 {
    let stop = cells(STOP_BUTTON);
    if view.heading.lead { cells(LEAD_BUTTON).saturating_add(1).saturating_add(stop) } else { stop }
}

fn fits(view: &View, width: u16) -> bool {
    let summary = summary(view).iter().fold(0u16, |n, span| n.saturating_add(cells(&span.content)));
    summary.saturating_add(1).saturating_add(buttons_width(view)) <= width
}

pub(super) fn height(view: &View, width: u16) -> u16 {
    if fits(view, width) { 3 } else { 4 }
}

fn buttons(slot: &Slot, view: &View) -> Vec<(Rect, &'static str, Hit)> {
    let row = if fits(view, slot.area.width) { 2 } else { 3 };
    let Some(y) = slot.y(row) else { return Vec::new() };
    let stop = cells(STOP_BUTTON).min(slot.area.width);
    let at = |x: u16, label: &'static str, hit: Hit| {
        (Rect { x: slot.area.x + x, y, width: cells(label), height: 1 }.intersection(slot.area), label, hit)
    };
    let x = slot.area.width - stop;
    let mut out = vec![at(x, STOP_BUTTON, Hit::Stop)];
    if view.heading.lead {
        let lead = cells(LEAD_BUTTON);
        out.push(at(x.saturating_sub(lead.saturating_add(1)), LEAD_BUTTON, Hit::Role(ORCHESTRATOR.into())));
    }
    out
}

pub(super) fn hit(slot: &Slot, view: &View, pos: Position) -> Option<Hit> {
    buttons(slot, view).into_iter().find(|(rect, ..)| rect.contains(pos)).map(|(.., hit)| hit)
}

fn segments(width: u16, n: usize) -> Vec<(u16, u16)> {
    let n = u16::try_from(n).unwrap_or(u16::MAX).max(1);
    let room = width.saturating_sub(n - 1);
    let (each, extra) = (room / n, room % n);
    let mut x = 0;
    (0..n)
        .map(|i| {
            let len = each + u16::from(i < extra);
            let at = (x, len);
            x += len + 1;
            at
        })
        .collect()
}

fn draw_bar(buf: &mut Buffer, slot: &Slot, view: &View) {
    let lit = reached(view);
    let line = Style::default().fg(line_colour(view.diagram.light));
    let brand = Style::default().fg(BRAND_COLOR);
    for (i, (x, len)) in segments(slot.area.width, view.diagram.stages.len()).into_iter().enumerate() {
        let on = lit || view.heading.active.is_some_and(|active| i <= active);
        let (glyph, style) = if on { ("━", brand) } else { ("─", line) };
        slot.put(buf, (1, x), &glyph.repeat(usize::from(len)), style);
    }
}

pub(super) fn draw(buf: &mut Buffer, slot: &Slot, view: &View, look: Look) {
    let heading = &view.heading;
    let messages = if heading.messages == 1 { "message" } else { "messages" };
    let right = format!("run {} · {} {messages}", heading.run, heading.messages);
    let width = slot.area.width;
    let tail =
        if cells(&right).saturating_add(9) <= width { slot.put_right(buf, 0, &right, dim(look.muted)) } else { 0 };
    let room = usize::from(width.saturating_sub(tail.saturating_add(1)));
    slot.put(buf, (0, 0), &text::truncate_cells(&heading.place, room), Style::default());
    draw_bar(buf, slot, view);
    let mut x = 0;
    for span in summary(view) {
        slot.put(buf, (2, x), &span.content, span.style);
        x = x.saturating_add(cells(&span.content));
    }
    for (rect, label, _) in buttons(slot, view) {
        let lit = look.hover.is_some_and(|pos| rect.contains(pos));
        put(buf, (rect.x, rect.y), label, action_style(lit), slot.area.right());
    }
}
