use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use super::{draw_close, hovered_at as hovered, put};
use crate::recap::text::{Block, Kind};

const TITLE: &str = "Recap";
const CHOICES: [&str; 3] = ["now", "yesterday", "7 days"];
const SEPARATOR: &str = " · ";
const REFRESH: &str = "↻";
const RETRY: &str = " retry ";
const BUTTON_WIDTH: u16 = 3;
const BODY_FROM: u16 = 3;
const BULLET: &str = "▸ ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum State {
    Idle,
    Running(String),
    Failed(String),
    Empty(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct View {
    pub(crate) choice: usize,
    pub(crate) line: String,
    pub(crate) state: State,
    pub(crate) blocks: Vec<Block>,
    pub(crate) scroll: usize,
    pub(crate) muted: Color,
    pub(crate) busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hit {
    Close,
    Refresh,
    Retry,
    Choice(usize),
    Link(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Run {
    pub(crate) x: u16,
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) quote: bool,
    pub(crate) heading: bool,
    pub(crate) link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Line {
    pub(crate) bullet: bool,
    pub(crate) runs: Vec<Run>,
}

pub(super) fn header(area: Rect) -> Rect {
    Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1).intersection(area)
}

pub(crate) fn close(area: Rect) -> Rect {
    let row = header(area);
    Rect::new(row.right().saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

pub(crate) fn refresh(area: Rect) -> Rect {
    let row = header(area);
    Rect::new(close(area).x.saturating_sub(BUTTON_WIDTH), row.y, BUTTON_WIDTH, 1).intersection(row)
}

fn fitted(text: &str, width: u16) -> String {
    crate::text::truncate_right(text, usize::from(width))
}

fn width_of(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

pub(crate) fn choices(area: Rect) -> [Rect; 3] {
    let row = header(area);
    let mut x = row.x + width_of(TITLE) + 3;
    CHOICES.map(|name| {
        let rect = Rect::new(x, row.y, width_of(name), 1).intersection(row);
        x = x.saturating_add(width_of(name) + width_of(SEPARATOR));
        rect
    })
}

pub(crate) fn retry(area: Rect, reason: &str) -> Rect {
    let row = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1).intersection(area);
    let shown = fitted(reason, row.width.saturating_sub(width_of(RETRY) + 1));
    let x = row.x + width_of(&shown) + 1;
    Rect::new(x, row.y, width_of(RETRY), 1).intersection(row)
}

pub(crate) fn body(area: Rect) -> Rect {
    let y = area.y.saturating_add(BODY_FROM).min(area.bottom());
    Rect::new(area.x + 1, y, area.width.saturating_sub(2), area.bottom() - y)
}

struct Frag {
    text: String,
    bold: bool,
    quote: bool,
    link: Option<String>,
}

fn words(block: &Block) -> Vec<Vec<Frag>> {
    let mut found: Vec<Vec<Frag>> = Vec::new();
    let mut current: Vec<Frag> = Vec::new();
    let flush = |current: &mut Vec<Frag>, found: &mut Vec<Vec<Frag>>| {
        if !current.is_empty() {
            found.push(std::mem::take(current));
        }
    };
    for segment in &block.segments {
        if segment.text.starts_with(char::is_whitespace) {
            flush(&mut current, &mut found);
        }
        for (i, piece) in segment.text.split_whitespace().enumerate() {
            if i > 0 {
                flush(&mut current, &mut found);
            }
            current.push(Frag {
                text: piece.to_string(),
                bold: segment.bold,
                quote: segment.quote,
                link: segment.link.clone(),
            });
        }
        if segment.text.ends_with(char::is_whitespace) {
            flush(&mut current, &mut found);
        }
    }
    flush(&mut current, &mut found);
    found
}

fn place(line: &mut Line, x: u16, word: &[Frag], heading: bool) {
    let mut at = x;
    for frag in word {
        line.runs.push(Run {
            x: at,
            text: frag.text.clone(),
            bold: frag.bold,
            quote: frag.quote,
            heading,
            link: frag.link.clone(),
        });
        at += width_of(&frag.text);
    }
}

fn wrap_block(block: &Block, width: u16) -> Vec<Line> {
    if block.kind == Kind::Blank {
        return vec![Line::default()];
    }
    let (first, indent) = if block.kind == Kind::Bullet { (width_of(BULLET), width_of(BULLET)) } else { (0, 0) };
    let heading = block.kind == Kind::Heading;
    let mut lines = vec![Line { bullet: block.kind == Kind::Bullet, runs: Vec::new() }];
    let mut x = first;
    for word in words(block) {
        let length: u16 = word.iter().map(|frag| width_of(&frag.text)).sum();
        let empty = lines.last().is_some_and(|line| line.runs.is_empty());
        if !empty && x + 1 + length > width {
            lines.push(Line::default());
            x = indent;
        } else if !empty {
            x += 1;
        }
        if let Some(line) = lines.last_mut() {
            place(line, x, &word, heading);
        }
        x += length;
    }
    lines
}

pub(crate) fn lines(width: u16, blocks: &[Block]) -> Vec<Line> {
    blocks.iter().flat_map(|block| wrap_block(block, width.max(8))).collect()
}

pub(crate) fn hit(area: Rect, view: &View, pos: Position) -> Option<Hit> {
    if close(area).contains(pos) {
        return Some(Hit::Close);
    }
    if refresh(area).contains(pos) {
        return Some(Hit::Refresh);
    }
    if let Some(i) = choices(area).iter().position(|rect| rect.contains(pos)) {
        return Some(Hit::Choice(i));
    }
    if let State::Failed(reason) = &view.state
        && retry(area, reason).contains(pos)
    {
        return Some(Hit::Retry);
    }
    link_at(area, view, pos).map(Hit::Link)
}

fn link_at(area: Rect, view: &View, pos: Position) -> Option<String> {
    let rect = body(area);
    if !rect.contains(pos) {
        return None;
    }
    let all = lines(rect.width, &view.blocks);
    let line = all.get(view.scroll + usize::from(pos.y - rect.y))?;
    let column = pos.x - rect.x;
    line.runs
        .iter()
        .find(|run| column >= run.x && column < run.x + width_of(&run.text) + 1 && run.link.is_some())?
        .link
        .clone()
}

pub(crate) fn total(area: Rect, view: &View) -> usize {
    lines(body(area).width, &view.blocks).len()
}

pub(crate) fn draw(f: &mut Frame, area: Rect, view: &View, hover: Option<Position>) {
    draw_header(f.buffer_mut(), area, view, hover);
    draw_status(f.buffer_mut(), area, view, hover);
    draw_body(f.buffer_mut(), area, view, hover);
}

fn draw_header(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let row = header(area);
    put(buf, (row.x, row.y), TITLE, Style::default().add_modifier(Modifier::BOLD), row.right());
    let rects = choices(area);
    for (i, (name, rect)) in CHOICES.iter().zip(rects).enumerate() {
        let style = if i == view.choice {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if hovered(hover, rect) {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(view.muted)
        };
        put(buf, (rect.x, rect.y), name, style, row.right());
        if i + 1 < CHOICES.len() {
            put(buf, (rect.right(), rect.y), SEPARATOR, Style::default().fg(view.muted), row.right());
        }
    }
    let refresh = refresh(area);
    let style =
        if hovered(hover, refresh) { Style::default().fg(Color::Cyan) } else { Style::default().fg(view.muted) };
    put(buf, (refresh.x + 1, refresh.y), REFRESH, style, refresh.right());
    draw_close(buf, close(area), hover, view.muted);
}

fn draw_status(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let row = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1).intersection(area);
    let muted = Style::default().fg(view.muted);
    match &view.state {
        State::Failed(reason) => {
            let reason = &fitted(reason, row.width.saturating_sub(width_of(RETRY) + 1));
            put(buf, (row.x, row.y), reason, Style::default().fg(Color::Red), row.right());
            let button = retry(area, reason);
            let style = if hovered(hover, button) {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().fg(Color::Cyan)
            };
            put(buf, (button.x, button.y), RETRY, style, row.right());
        }
        State::Running(text) => {
            put(buf, (row.x, row.y), &fitted(text, row.width), Style::default().fg(Color::Cyan), row.right());
        }
        State::Empty(_) | State::Idle => {
            put(buf, (row.x, row.y), &fitted(&view.line, row.width), muted, row.right());
        }
    }
}

fn run_style(run: &Run, hovering: bool, muted: Color) -> Style {
    let mut style = Style::default();
    if run.link.is_some() {
        style = style.fg(Color::Cyan);
        if hovering {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
    } else if run.quote {
        style = style.fg(muted);
    }
    if run.bold || run.heading {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

fn draw_body(buf: &mut Buffer, area: Rect, view: &View, hover: Option<Position>) {
    let rect = body(area);
    if let State::Empty(text) = &view.state {
        put(buf, (rect.x, rect.y), text, Style::default().fg(view.muted), rect.right());
        return;
    }
    let all = lines(rect.width, &view.blocks);
    let shown = all.iter().skip(view.scroll).take(usize::from(rect.height));
    for (offset, line) in shown.enumerate() {
        let y = rect.y + u16::try_from(offset).unwrap_or(0);
        draw_line(buf, line, (rect.x, y, rect.right()), hover, view.muted);
    }
    let below = all.len().saturating_sub(view.scroll + usize::from(rect.height));
    if below > 0 && rect.height > 0 {
        let text = format!(" ↓ {below} more ");
        put(
            buf,
            (rect.right().saturating_sub(width_of(&text)), rect.bottom() - 1),
            &text,
            Style::default().fg(view.muted),
            rect.right(),
        );
    }
}

fn draw_line(buf: &mut Buffer, line: &Line, (x, y, end): (u16, u16, u16), hover: Option<Position>, muted: Color) {
    if line.bullet {
        put(buf, (x, y), BULLET, Style::default().fg(muted), end);
    }
    for run in &line.runs {
        let rect = Rect::new(x + run.x, y, width_of(&run.text), 1);
        put(buf, (rect.x, y), &run.text, run_style(run, hovered(hover, rect), muted), end);
    }
}

#[cfg(test)]
mod tests;
