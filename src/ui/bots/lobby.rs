use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::editor::wrap_text;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::{button_style, sidebar_hovered};

pub(crate) const IDEAS: [(&str, &str); 3] = [
    ("PRs", "Every weekday at 9:00, review my open pull requests and tell me which are blocked"),
    ("News", "Every morning at 8:00, sum up the news about the tools I use"),
    ("Releases", "Every Monday, check the new releases of the tools I use and tell me what changed"),
];

pub(crate) const NEW_BOT: &str = "+ New bot";

const WIDTH: u16 = 72;
const MARGIN: u16 = 2;
const NAME_COLUMN: usize = 10;
const WHAT: &str = "A bot is an agent of your own, outside any project: a name, a folder, a memory and a chat.";
const SETS_UP: &str = "Tell it what it is for and it sets itself up, schedules its own work and answers here.";
const PICK: &str = "Pick a bot in the list to read its chat and talk to it.";
const FROM_AN_IDEA: &str = "Start from an idea";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lobby {
    Empty,
    Pick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LobbyHit {
    New,
    Idea(usize),
}

struct Row {
    line: Line<'static>,
    hit: Option<(LobbyHit, Range<u16>)>,
}

impl Row {
    fn plain(line: Line<'static>) -> Self {
        Self { line, hit: None }
    }

    fn blank() -> Self {
        Self::plain(Line::default())
    }
}

impl Lobby {
    fn heading(self) -> &'static str {
        match self {
            Self::Empty => "Meet your bots",
            Self::Pick => "Your bots",
        }
    }

    fn lines(self) -> &'static [&'static str] {
        match self {
            Self::Empty => &[WHAT, SETS_UP],
            Self::Pick => &[PICK],
        }
    }
}

fn column(pane: Rect) -> Rect {
    let width = pane.width.saturating_sub(2 * MARGIN).min(WIDTH);
    Rect { x: pane.x + (pane.width - width) / 2, width, ..pane }
}

fn centred(text: &str, width: usize, style: Style) -> Line<'static> {
    let lead = width.saturating_sub(text.width()) / 2;
    Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(text.to_string(), style)])
}

fn new_button(width: usize) -> Row {
    let label = format!(" {NEW_BOT} ");
    let lead = width.saturating_sub(label.width()) / 2;
    let start = u16::try_from(lead).unwrap_or(u16::MAX);
    let end = start.saturating_add(u16::try_from(label.width()).unwrap_or(u16::MAX));
    let line = Line::from(vec![Span::raw(" ".repeat(lead)), Span::styled(label, Style::default().fg(Color::Cyan))]);
    Row { line, hit: Some((LobbyHit::New, start..end)) }
}

fn idea_rows(at: usize, (name, about): (&str, &str), width: usize, soft: Color) -> Vec<Row> {
    let hit = Some((LobbyHit::Idea(at), 0..u16::MAX));
    let bold = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let room = width.saturating_sub(NAME_COLUMN).max(1);
    let lead = |first: bool| {
        let name = if first { truncate_right(name, NAME_COLUMN - 1) } else { String::new() };
        Span::styled(format!("{name:<NAME_COLUMN$}"), bold)
    };
    let rows = wrap_text(about, room).into_iter().enumerate().map(|(i, text)| {
        let line = Line::from(vec![lead(i == 0), Span::styled(text, Style::default().fg(soft))]);
        Row { line, hit: hit.clone() }
    });
    rows.collect()
}

#[derive(Clone, Copy)]
struct Colours {
    muted: Color,
    soft: Color,
}

impl Colours {
    const PLAIN: Self = Self { muted: Color::Reset, soft: Color::Reset };
}

fn rows(lobby: Lobby, width: u16, colours: Colours) -> Vec<Row> {
    let width = usize::from(width);
    let dim = Style::default().fg(colours.muted);
    let mut rows = vec![Row::plain(centred(lobby.heading(), width, Style::default().add_modifier(Modifier::BOLD)))];
    rows.push(Row::blank());
    for text in lobby.lines().iter().flat_map(|text| wrap_text(text, width.max(1))) {
        rows.push(Row::plain(centred(text.trim_end(), width, dim)));
    }
    rows.extend([Row::blank(), new_button(width)]);
    if lobby == Lobby::Empty {
        rows.extend([
            Row::blank(),
            Row::plain(Line::from(Span::styled(FROM_AN_IDEA, dim.add_modifier(Modifier::BOLD)))),
        ]);
        rows.push(Row::blank());
        for (at, idea) in IDEAS.into_iter().enumerate() {
            rows.extend(idea_rows(at, idea, width, colours.soft));
        }
    }
    rows
}

fn top(pane: Rect, count: usize) -> u16 {
    let count = u16::try_from(count).unwrap_or(u16::MAX).min(pane.height);
    pane.y + (pane.height - count) / 2
}

fn row_rect(column: Rect, y: u16) -> Rect {
    Rect { y, height: 1, ..column }.intersection(column)
}

fn hit_rect(column: Rect, y: u16, cols: &Range<u16>) -> Rect {
    let x = column.x.saturating_add(cols.start);
    let width = cols.end.min(column.width).saturating_sub(cols.start);
    Rect { x, width, ..row_rect(column, y) }.intersection(column)
}

fn placed(lobby: Lobby, pane: Rect, colours: Colours) -> Vec<(Rect, Row)> {
    let column = column(pane);
    let rows = rows(lobby, column.width, colours);
    let first = top(pane, rows.len());
    let ys = (first..pane.bottom()).map(|y| row_rect(column, y));
    ys.zip(rows).collect()
}

pub(crate) fn lobby_hit(lobby: Lobby, pane: Rect, pos: Position) -> Option<LobbyHit> {
    let column = column(pane);
    placed(lobby, pane, Colours::PLAIN).into_iter().find_map(|(r, row)| {
        let (hit, cols) = row.hit?;
        hit_rect(column, r.y, &cols).contains(pos).then_some(hit)
    })
}

fn idea_span(placed: &[(Rect, Row)], idea: LobbyHit) -> Rect {
    let rects = placed.iter().filter(|(_, row)| row.hit.as_ref().is_some_and(|(hit, _)| *hit == idea));
    rects.map(|(r, _)| *r).reduce(Rect::union).unwrap_or_default()
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, lobby: Lobby, pane: Rect) {
    let column = column(pane);
    let placed = placed(lobby, pane, Colours { muted: view.muted, soft: view.soft });
    for at in 0..IDEAS.len() {
        let span = idea_span(&placed, LobbyHit::Idea(at));
        if sidebar_hovered(view, span) {
            f.buffer_mut().set_style(span, Style::default().bg(view.hover_fill()));
        }
    }
    for (r, row) in &placed {
        let mut line = row.line.clone();
        if let Some((LobbyHit::New, cols)) = &row.hit
            && let Some(label) = line.spans.last_mut()
        {
            let button = hit_rect(column, r.y, cols);
            label.style = button_style(view, button, Style::default().fg(Color::Cyan), Color::Cyan);
        }
        f.render_widget(Paragraph::new(line), *r);
    }
}
