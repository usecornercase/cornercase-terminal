use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::flows::{draw_buttons, draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::settings::settings_area;
use super::{Note, form_inner};
use crate::bots::asks::{self, Form, Hit};
use crate::editor::{Editor, wrap_text};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::widgets::overlay_block;

const FIELD_WIDTH: u16 = 52;
const QUOTE: &str = "│ ";
const HINT: &str = "tab moves to the next field · enter saves · esc is not now";
const REPLACES: &str = " replaces the saved one";

pub(crate) struct BotAsk {
    pub(crate) title: String,
    pub(crate) agent: String,
    pub(crate) access: String,
    pub(crate) full: bool,
    pub(crate) about: String,
    pub(crate) fields: Vec<AskField>,
    pub(crate) focus: usize,
    pub(crate) error: Option<String>,
}

pub(crate) struct AskField {
    pub(crate) name: String,
    pub(crate) value: Editor,
    pub(crate) replaces: bool,
}

struct Geometry {
    head: Option<Rect>,
    says: Vec<(Rect, String)>,
    fields: Vec<(Rect, Rect)>,
    what: Vec<(Rect, String)>,
    note: Rect,
    buttons: Rect,
}

fn row(inner: Rect, y: u16) -> Rect {
    Rect::new(inner.x, y, inner.width, 1).intersection(inner)
}

fn rows_of(inner: Rect, y: &mut u16, texts: Vec<String>) -> Vec<(Rect, String)> {
    texts
        .into_iter()
        .map(|text| {
            let at = row(inner, *y);
            *y = y.saturating_add(1);
            (at, text)
        })
        .collect()
}

struct Room {
    head: bool,
    says: usize,
    what: usize,
    pitch: u16,
}

fn room(height: usize, fields: usize, about: usize, what: usize) -> Room {
    let pitch = if height >= fields * 3 { 3 } else { 2 };
    let mut left = height.saturating_sub(fields * usize::from(pitch));
    let head = left >= 2;
    left = left.saturating_sub(if head { 2 } else { 0 });
    let says = (about + 1).min(left.saturating_sub(1));
    left = left.saturating_sub(says + usize::from(says > 0));
    Room { head, says, what: what.min(left), pitch }
}

fn geometry(area: Rect, view: &BotAsk) -> Geometry {
    let inner = form_inner(settings_area(area));
    let width = usize::from(inner.width.max(1));
    let about = wrap_text(&view.about, width.saturating_sub(QUOTE.chars().count()).max(1));
    let what = wrap_text(asks::WHAT_HAPPENS, width);
    let height = usize::from(inner.height.saturating_sub(2));
    let room = room(height, view.fields.len(), about.len(), what.len());
    let mut y = inner.y;
    let head = room.head.then(|| row(inner, y));
    y = y.saturating_add(if room.head { 2 } else { 0 });
    let mut says_texts: Vec<String> = Vec::new();
    if room.says > 0 {
        says_texts.push(String::from("It says:"));
        says_texts.extend(about.into_iter().take(room.says - 1));
    }
    let says = rows_of(inner, &mut y, says_texts);
    y = y.saturating_add(u16::from(!says.is_empty()));
    let field_width = inner.width.min(FIELD_WIDTH);
    let fields = (0..view.fields.len())
        .map(|_| {
            let label = row(inner, y);
            let field = Rect::new(inner.x, y.saturating_add(1), field_width, 1).intersection(inner);
            y = y.saturating_add(room.pitch);
            (label, field)
        })
        .collect();
    let what = rows_of(inner, &mut y, what.into_iter().take(room.what).collect());
    let bottom = inner.bottom();
    Geometry {
        head,
        says,
        fields,
        what,
        note: row(inner, bottom.saturating_sub(2)),
        buttons: row(inner, bottom.saturating_sub(1)),
    }
}

pub(crate) fn text(area: Rect, view: &BotAsk, at: usize) -> Rect {
    geometry(area, view).fields.get(at).map_or_else(Rect::default, |(_, field)| text_inside(*field))
}

pub(crate) fn hit(area: Rect, view: &BotAsk, pos: Position) -> Option<Hit> {
    let shapes = geometry(area, view);
    let labels = Form::buttons();
    if let Some(i) = issue_buttons_in(shapes.buttons, &labels).iter().position(|r| r.contains(pos)) {
        return Some(Hit::Button(labels[i]));
    }
    shapes.fields.iter().enumerate().find_map(|(at, (_, field))| {
        let col = usize::from(pos.x.saturating_sub(text_inside(*field).x));
        field.contains(pos).then_some(Hit::Field { at, col })
    })
}

fn draw_head(f: &mut Frame, view: &View, ask: &BotAsk, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let access = if ask.full { Style::default().fg(Color::Red) } else { dim };
    let line =
        Line::from(vec![Span::styled(format!("{} · ", ask.agent), dim), Span::styled(ask.access.clone(), access)]);
    f.render_widget(Paragraph::new(line), r);
}

fn draw_rows(f: &mut Frame, rows: &[(Rect, String)], style: Style) {
    for (r, text) in rows {
        f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(r.width)), style)), *r);
    }
}

fn draw_field(
    f: &mut Frame,
    view: &View,
    field: &AskField,
    (label, at): (Rect, Rect),
    focused: bool,
) -> Option<Position> {
    let dim = Style::default().fg(view.muted);
    let name = if focused { Style::default().fg(Color::Cyan) } else { Style::default() };
    let mut spans = vec![Span::styled(field.name.clone(), name)];
    if field.replaces {
        spans.push(Span::styled(REPLACES, dim));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), label);
    draw_text(f, view, at, &field.value, focused)
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, ask: &BotAsk) {
    let screen = f.area();
    let r = settings_area(screen);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &ask.title), r);
    let shapes = geometry(screen, ask);
    let dim = Style::default().fg(view.muted);
    if let Some(head) = shapes.head {
        draw_head(f, view, ask, head);
    }
    if let Some(((first, label), rest)) = shapes.says.split_first() {
        draw_rows(f, &[(*first, label.clone())], dim);
        for (r, text) in rest {
            let text = truncate_right(text, usize::from(r.width).saturating_sub(QUOTE.chars().count()));
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled(QUOTE, dim), Span::raw(text)])), *r);
        }
    }
    let mut cursor = None;
    for (at, (field, rects)) in ask.fields.iter().zip(&shapes.fields).enumerate() {
        cursor = draw_field(f, view, field, *rects, at == ask.focus).or(cursor);
    }
    draw_rows(f, &shapes.what, dim);
    let note = ask.error.clone().map(Note::Error);
    draw_hint(f, view, note.as_ref(), HINT, shapes.note);
    let labels = Form::buttons();
    draw_buttons(f, view, &issue_buttons_in(shapes.buttons, &labels), &labels, asks::SAVE);
    if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
