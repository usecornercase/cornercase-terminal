use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::dialog::{dialog_button_style, pick_background};
use super::remote_access::{Linked, rows, sized, text, with_code};
use super::widgets::{button_width, update_button};
use super::{View, draw_submit, hovered, overlay_block};
use crate::settings::DONE;
use crate::text::truncate_right;

pub(crate) const TITLE: &str = "Discord forum";
pub(crate) const TURN_OFF: &str = "turn off";
const DONE_MARK: &str = "✓";
const TODO_MARK: &str = "○";
const PICK: &str = "pick the server for the forum:";
const SERVER: &str = "  › ";

pub(crate) struct ForumSetup {
    pub(crate) steps: Vec<(bool, String)>,
    pub(crate) servers: Vec<String>,
    pub(crate) url: Option<String>,
    pub(crate) code: Vec<Line<'static>>,
    pub(crate) notes: Vec<String>,
    pub(crate) problem: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForumHit {
    Done,
    TurnOff,
    Server(usize),
}

fn step(done: bool, label: &str, width: u16, muted: Color) -> Line<'static> {
    let (mark, colour) = if done { (DONE_MARK, Color::Green) } else { (TODO_MARK, muted) };
    let label = truncate_right(label, usize::from(width).saturating_sub(2));
    Line::from(vec![Span::styled(mark, Style::default().fg(colour)), Span::raw(format!(" {label}"))])
}

fn server(name: &str, width: u16) -> Line<'static> {
    let room = usize::from(width).saturating_sub(SERVER.chars().count());
    Line::from(format!("{SERVER}{}", truncate_right(name, room)))
}

fn body(setup: &ForumSetup, width: u16, room: u16, muted: Color) -> (Vec<Line<'static>>, usize) {
    let mut lines: Vec<Line<'static>> =
        setup.steps.iter().map(|(done, label)| step(*done, label, width, muted)).collect();
    if let Some(problem) = &setup.problem {
        lines.push(Line::default());
        text(&mut lines, problem, Style::default().fg(Color::Red), width);
    }
    let mut first = lines.len();
    if !setup.servers.is_empty() {
        lines.push(Line::default());
        text(&mut lines, PICK, Style::default().fg(muted), width);
        first = lines.len();
        lines.extend(setup.servers.iter().map(|name| server(name, width)));
    }
    if let Some(url) = &setup.url {
        lines.push(Line::default());
        let room = usize::from(room).saturating_sub(lines.len());
        let linked = Linked { url, code: &setup.code, notes: &setup.notes };
        lines.extend(with_code(&linked, width, room, muted));
    }
    (lines, first)
}

pub(crate) fn area(screen: Rect, setup: &ForumSetup) -> Rect {
    sized(screen, |inner, room| body(setup, inner, room, Color::Reset).0.len())
}

fn buttons(screen: Rect, setup: &ForumSetup) -> [Rect; 2] {
    let row = rows(area(screen, setup))[2];
    let done = update_button(row, DONE);
    let width = button_width(TURN_OFF);
    let off = Rect { x: done.x.saturating_sub(width + 1), width, ..row }.intersection(row);
    [off, done]
}

fn server_rows(screen: Rect, setup: &ForumSetup) -> Vec<Rect> {
    let text = rows(area(screen, setup))[0];
    let (_, first) = body(setup, text.width, screen.height.saturating_sub(4), Color::Reset);
    (0..setup.servers.len())
        .map(|i| {
            let y = u16::try_from(first + i).map_or(u16::MAX, |row| text.y.saturating_add(row));
            Rect::new(text.x, y, text.width, 1).intersection(text)
        })
        .collect()
}

pub(crate) fn hit(screen: Rect, setup: &ForumSetup, pos: Position) -> Option<ForumHit> {
    let [off, done] = buttons(screen, setup);
    if done.contains(pos) {
        return Some(ForumHit::Done);
    }
    if off.contains(pos) {
        return Some(ForumHit::TurnOff);
    }
    server_rows(screen, setup).iter().position(|row| row.contains(pos)).map(ForumHit::Server)
}

pub(crate) fn draw(f: &mut Frame, view: &View, setup: &ForumSetup) {
    let r = area(f.area(), setup);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, TITLE), r);
    let [text, _, _] = rows(r);
    let room = f.area().height.saturating_sub(4);
    f.render_widget(Paragraph::new(body(setup, text.width, room, view.muted).0), text);
    for row in server_rows(f.area(), setup).into_iter().filter(|row| hovered(view, *row)) {
        f.buffer_mut().set_style(row, pick_background(view, false, row));
    }
    let [off, done] = buttons(f.area(), setup);
    let style = dialog_button_style(view, off, false);
    f.render_widget(Paragraph::new(Span::styled(format!(" {TURN_OFF} "), style)), off);
    draw_submit(f, view, done, DONE);
}
