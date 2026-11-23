use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{draw_buttons, draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::{Note, form_inner};
use crate::bots::memory::{BUTTONS, CANCEL, SAVE};
use crate::editor::{Editor, wrap_text};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;
const INTRO_ROWS: u16 = 2;

pub(crate) struct BotMemory {
    pub(crate) title: String,
    pub(crate) intro: String,
    pub(crate) text: Editor,
    pub(crate) text_focused: bool,
    pub(crate) note: Option<Note>,
    pub(crate) hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Text(usize, usize),
    Cancel,
    Save,
}

pub(crate) fn area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(WIDTH);
    let height = area.height.saturating_sub(2).min(HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn rows(screen: Rect) -> [Rect; 4] {
    let [intro, _, field, _, hint, buttons] = Layout::vertical([
        Constraint::Length(INTRO_ROWS),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(area(screen)));
    [intro, field, hint, buttons]
}

pub(crate) fn field(screen: Rect) -> Rect {
    rows(screen)[1]
}

pub(crate) fn text(screen: Rect) -> Rect {
    text_inside(field(screen))
}

fn buttons(screen: Rect) -> Vec<Rect> {
    issue_buttons_in(rows(screen)[3], &BUTTONS)
}

pub(crate) fn hit(screen: Rect, pos: Position) -> Option<Hit> {
    let at = buttons(screen).iter().position(|r| r.contains(pos)).and_then(|i| BUTTONS.get(i).copied());
    match at {
        Some(CANCEL) => return Some(Hit::Cancel),
        Some(SAVE) => return Some(Hit::Save),
        _ => {}
    }
    let text = text(screen);
    field(screen)
        .contains(pos)
        .then(|| Hit::Text(usize::from(pos.y.saturating_sub(text.y)), usize::from(pos.x.saturating_sub(text.x))))
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, memory: &BotMemory) {
    let screen = f.area();
    let r = area(screen);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &memory.title), r);
    let [intro, field, hint, _] = rows(screen);
    let dim = Style::default().fg(view.muted);
    for (n, line) in
        wrap_text(&memory.intro, usize::from(intro.width.max(1))).iter().take(usize::from(INTRO_ROWS)).enumerate()
    {
        let y = intro.y.saturating_add(u16::try_from(n).unwrap_or(u16::MAX));
        let shown = truncate_right(line, usize::from(intro.width));
        f.render_widget(Paragraph::new(Span::styled(shown, dim)), Rect { y, height: 1, ..intro });
    }
    let cursor = draw_text(f, view, field, &memory.text, memory.text_focused);
    draw_hint(f, view, memory.note.as_ref(), &memory.hint, hint);
    let rects = buttons(screen);
    draw_buttons(f, view, &rects, &BUTTONS, SAVE);
    if !memory.text_focused
        && let Some(save) = rects.last()
    {
        let lit = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
        f.buffer_mut().set_style(*save, lit);
    }
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
