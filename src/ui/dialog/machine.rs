use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::Note;
use super::flows::{draw_buttons, draw_hint, draw_text, text_inside};
use super::issues::issue_buttons_in;
use super::settings::settings_area;
use super::share_settings::rows;
use crate::machines::form::{Field, Form, Hit};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::overlay_block;

const LABEL: u16 = 14;
const NAME: u16 = 24;
const ICON: u16 = 18;
const WIDE: u16 = 52;
const PITCH: usize = 3;
const INTRO: &str = "Another computer you reach with ssh. cornercase keeps a link to its server, starts one there \
                     when none runs, and never stops it: removing the machine here leaves its server running.";

fn line(area: Rect, field: Field) -> Rect {
    let body = rows(area)[1];
    let at = Field::ALL.iter().position(|f| *f == field).unwrap_or_default() * PITCH;
    let y = body.y.saturating_add(u16::try_from(at).unwrap_or(u16::MAX));
    Rect::new(body.x, y, body.width, 2).intersection(body)
}

pub(crate) fn field(area: Rect, field: Field) -> Rect {
    let r = line(area, field);
    let wanted = match field {
        Field::Name => NAME,
        Field::Icon => ICON,
        Field::Destination | Field::Folder => WIDE,
    };
    let width = wanted.min(r.width.saturating_sub(LABEL));
    Rect::new(r.x.saturating_add(LABEL), r.y, width, r.height.min(1))
}

pub(crate) fn text(area: Rect, field: Field) -> Rect {
    text_inside(self::field(area, field))
}

fn note(area: Rect, field: Field) -> Rect {
    let r = line(area, field);
    let x = LABEL + 1;
    Rect::new(r.x.saturating_add(x), r.y.saturating_add(1), r.width.saturating_sub(x), 1).intersection(r)
}

pub(crate) fn list(area: Rect, form: &Form) -> Option<Rect> {
    form.icon.is_open().then(|| combo::list(field(area, Field::Icon), &form.icon, area))
}

pub(crate) fn area_with_list(area: Rect, form: &Form) -> Rect {
    list(area, form).map_or_else(|| settings_area(area), |list| settings_area(area).union(list))
}

pub(crate) fn buttons(area: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(rows(area)[3], labels)
}

pub(crate) fn hit(area: Rect, form: &Form, pos: Position) -> Option<Hit> {
    if let Some(list) = list(area, form) {
        return Some(combo::hit(list, &form.icon, pos).map_or(Hit::Close, Hit::Choose));
    }
    let labels = form.buttons();
    if let Some(i) = buttons(area, &labels).iter().position(|r| r.contains(pos)) {
        return Some(Hit::Button(labels[i]));
    }
    Field::ALL.into_iter().find_map(|f| {
        let t = text(area, f);
        let (line, col) = (usize::from(pos.y.saturating_sub(t.y)), usize::from(pos.x.saturating_sub(t.x)));
        field(area, f).contains(pos).then_some(Hit::Field { field: f, line, col })
    })
}

fn draw_field(f: &mut Frame, view: &View, form: &Form, field: Field) -> Option<Position> {
    let screen = f.area();
    let r = line(screen, field);
    let dim = Style::default().fg(view.muted);
    let focused = form.focus == field;
    let label = if focused { Style::default().fg(Color::Cyan) } else { dim };
    f.render_widget(Paragraph::new(Span::styled(field.label(), label)), Rect { width: LABEL, height: 1, ..r });
    let below = note(screen, field);
    let (about, style) = match form.error(field) {
        Some(why) => (why, Style::default().fg(Color::Red)),
        None => (field.about(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(about, usize::from(below.width)), style)), below);
    let at = self::field(screen, field);
    if let Some(editor) = form.editor_of(field) {
        return draw_text(f, view, at, editor, focused && !form.icon.is_open());
    }
    combo::draw_field(f, view, at, combo::Field { combo: &form.icon, placeholder: "", focused });
    None
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let r = settings_area(screen);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &form.title()), r);
    let [intro, _, hint, _] = rows(screen);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled(INTRO, dim)).wrap(Wrap { trim: true }), intro);
    let cursor = Field::ALL.into_iter().fold(None, |cursor, field| draw_field(f, view, form, field).or(cursor));
    let notice = form.notice.clone().map(Note::Error);
    draw_hint(f, view, notice.as_ref(), form.hint(), hint);
    let labels = form.buttons();
    draw_buttons(f, view, &buttons(screen, &labels), &labels, form.submit());
    if let Some(list) = list(screen, form) {
        combo::draw_list(f, view, list, &form.icon);
    } else if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
}
