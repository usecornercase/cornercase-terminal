use std::ops::Range;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::View;
use super::dialog::dialog_button_style;
use super::rows::draw_rail;
use super::widgets::{button_width, put};
use crate::editor::{Editor, wrap};

const WIDTH: u16 = 54;

const MARGIN: u16 = 1;

pub(crate) const FIELD_ROWS: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TourButton {
    Start,
    Idea,
    TurnOn,
    Docs,
    End,
    Back,
    Skip,
    Next,
    Done,
}

impl TourButton {
    fn primary(self) -> bool {
        matches!(self, Self::Start | Self::TurnOn | Self::Next | Self::Done)
    }
}

pub(crate) struct TourCard {
    pub(crate) title: String,
    pub(crate) step: String,
    pub(crate) text: String,
    pub(crate) note: Option<String>,
    pub(crate) field: Option<Editor>,
    pub(crate) focused: bool,
    pub(crate) agent: Option<String>,
    pub(crate) actions: Vec<TourButton>,
    pub(crate) back: bool,
    pub(crate) forward: TourButton,
    pub(crate) target: Rect,
    pub(crate) button: bool,
}

impl TourCard {
    pub(crate) fn label(&self, button: TourButton) -> String {
        match button {
            TourButton::Start => format!("start {}", self.agent.as_deref().unwrap_or("agent")),
            TourButton::Idea => "another idea".into(),
            TourButton::TurnOn => "turn on".into(),
            TourButton::Docs => "docs".into(),
            TourButton::End => "end tour".into(),
            TourButton::Back => "back".into(),
            TourButton::Skip => "skip".into(),
            TourButton::Next => "next".into(),
            TourButton::Done => "done".into(),
        }
    }

    fn nav(&self) -> (Vec<TourButton>, Vec<TourButton>) {
        let right = if self.back { vec![TourButton::Back, self.forward] } else { vec![self.forward] };
        (vec![TourButton::Docs, TourButton::End], right)
    }
}

pub(crate) struct TourLayout {
    pub(crate) area: Rect,
    pub(crate) text: Rect,
    pub(crate) field: Rect,
    pub(crate) buttons: Vec<(TourButton, Rect)>,
}

fn lines(text: &str, width: usize) -> Vec<Range<usize>> {
    if text.is_empty() { Vec::new() } else { wrap(text, width) }
}

fn widths(card: &TourCard, buttons: &[TourButton]) -> u16 {
    let total: u16 = buttons.iter().map(|&b| button_width(&card.label(b))).sum();
    total + u16::try_from(buttons.len().saturating_sub(1)).unwrap_or(0)
}

fn place(card: Rect, area: Rect, target: Rect) -> Rect {
    let (w, h) = (card.width, card.height);
    let right = area.right().saturating_sub(w + MARGIN).max(area.x);
    let left = area.x.saturating_add(MARGIN).min(right);
    let bottom = area.bottom().saturating_sub(h + MARGIN).max(area.y);
    let top = area.y.saturating_add(MARGIN).min(bottom);
    let spots = [(right, bottom), (left, bottom), (right, top), (left, top)];
    let fits = |&(x, y): &(u16, u16)| Rect::new(x, y, w, h).intersection(target).is_empty();
    let (x, y) = spots.iter().copied().find(|spot| fits(spot)).unwrap_or(spots[0]);
    Rect::new(x, y, w, h)
}

pub(crate) fn tour_layout(area: Rect, card: &TourCard) -> TourLayout {
    let width = WIDTH.min(area.width.saturating_sub(2 * MARGIN)).max(area.width.min(12));
    let inner_width = width.saturating_sub(4);
    let w = usize::from(inner_width);
    let text_rows = 1 + lines(&card.text, w).len() + card.note.as_deref().map_or(0, |n| lines(n, w).len());
    let text_rows = u16::try_from(text_rows).unwrap_or(u16::MAX);
    let field_rows = if card.field.is_some() { FIELD_ROWS + 1 } else { 0 };
    let (left, right) = card.nav();
    let one_row = widths(card, &left) + 2 + widths(card, &right) <= inner_width;
    let nav_rows = if one_row { 1 } else { 2 };
    let action_rows = u16::from(!card.actions.is_empty());
    let height = (text_rows + field_rows + 1 + action_rows + nav_rows + 2).min(area.height);
    let rect = place(Rect::new(0, 0, width, height), area, card.target);
    let inner = rect.inner(Margin::new(2, 1));
    let text = Rect { height: text_rows.min(inner.height), ..inner };
    let field = if card.field.is_some() {
        Rect { y: text.bottom() + 1, height: FIELD_ROWS, ..inner }.intersection(inner)
    } else {
        Rect::default()
    };
    let buttons = place_buttons(card, inner, one_row, action_rows > 0);
    TourLayout { area: rect, text, field, buttons }
}

fn place_buttons(card: &TourCard, inner: Rect, one_row: bool, actions: bool) -> Vec<(TourButton, Rect)> {
    let row = |from_bottom: u16| Rect { y: inner.bottom().saturating_sub(from_bottom), height: 1, ..inner };
    let (left, right) = card.nav();
    let mut buttons = Vec::new();
    put_right(card, row(1), &right, &mut buttons);
    if one_row {
        put_left(card, row(1), &left, &mut buttons);
    } else {
        put_right(card, row(2), &left, &mut buttons);
    }
    if actions {
        put_right(card, row(if one_row { 2 } else { 3 }), &card.actions, &mut buttons);
    }
    buttons.retain(|(_, r)| !r.is_empty() && inner.contains(Position::new(r.x, r.y)));
    buttons
}

fn put_right(card: &TourCard, row: Rect, labels: &[TourButton], buttons: &mut Vec<(TourButton, Rect)>) {
    let mut x = row.right();
    for &b in labels.iter().rev() {
        let bw = button_width(&card.label(b));
        x = x.saturating_sub(bw);
        buttons.push((b, Rect { x, width: bw, ..row }.intersection(row)));
        x = x.saturating_sub(1);
    }
}

fn put_left(card: &TourCard, row: Rect, labels: &[TourButton], buttons: &mut Vec<(TourButton, Rect)>) {
    let mut x = row.x;
    for &b in labels {
        let bw = button_width(&card.label(b));
        buttons.push((b, Rect { x, width: bw, ..row }.intersection(row)));
        x += bw + 1;
    }
}

pub(crate) fn tour_hit(area: Rect, card: &TourCard, pos: Position) -> Option<TourButton> {
    tour_layout(area, card).buttons.into_iter().find(|(_, r)| r.contains(pos)).map(|(b, _)| b)
}

pub(super) fn draw_pointer(f: &mut Frame, card: &TourCard) {
    let target = card.target.intersection(f.area());
    if target.is_empty() {
        return;
    }
    if card.button {
        let lit = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
        f.buffer_mut().set_style(target, lit);
    } else {
        draw_rail(f, target);
    }
}

fn draw_field(buf: &mut Buffer, view: &View, editor: &Editor, r: Rect, focused: bool) -> Option<Position> {
    let width = usize::from(r.width.saturating_sub(1)).max(1);
    buf.set_style(r, Style::default().bg(view.surface()));
    let text: Vec<char> = editor.text().chars().collect();
    let scroll = editor.scroll();
    for (i, range) in editor.lines(width).into_iter().skip(scroll).take(usize::from(r.height)).enumerate() {
        let line: String = text[range].iter().collect();
        let y = r.y + u16::try_from(i).unwrap_or(0);
        put(buf, (r.x, y), &line, Style::default().bg(view.surface()), r.right());
    }
    if !focused {
        return None;
    }
    let (line, col) = editor.position(width);
    let row = line.checked_sub(scroll).filter(|&l| l < usize::from(r.height))?;
    Some(Position::new(r.x + u16::try_from(col).ok()?, r.y + u16::try_from(row).ok()?))
}

pub(super) fn draw_tour(f: &mut Frame, view: &View, card: &TourCard) {
    let layout = tour_layout(f.area(), card);
    let r = layout.area;
    f.render_widget(Clear, r);
    let border = Style::default().fg(Color::Cyan);
    let title = Line::from(vec![
        Span::styled("─", border),
        Span::styled(format!(" {} ", card.title), Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD)),
    ]);
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(border).title(title);
    f.render_widget(block, r);
    let w = usize::from(layout.text.width);
    let mut text = vec![Line::from(Span::styled(card.step.clone(), Style::default().add_modifier(Modifier::BOLD)))];
    let chars: Vec<char> = card.text.chars().collect();
    text.extend(lines(&card.text, w).into_iter().map(|range| Line::from(chars[range].iter().collect::<String>())));
    if let Some(note) = &card.note {
        let chars: Vec<char> = note.chars().collect();
        let muted = Style::default().fg(view.muted);
        text.extend(
            lines(note, w)
                .into_iter()
                .map(|range| Line::from(Span::styled(chars[range].iter().collect::<String>(), muted))),
        );
    }
    f.render_widget(Paragraph::new(text), layout.text);
    if let Some(editor) = &card.field
        && let Some(cursor) = draw_field(f.buffer_mut(), view, editor, layout.field, card.focused)
    {
        f.set_cursor_position(cursor);
    }
    for (button, rect) in &layout.buttons {
        let style = dialog_button_style(view, *rect, button.primary());
        f.render_widget(Paragraph::new(Span::styled(format!(" {} ", card.label(*button)), style)), *rect);
    }
}
