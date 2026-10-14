use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{
    CANCEL_LABEL, FORM_WIDTH, INPUT_PROMPT, Note, buttons_in, draw_dialog_buttons, draw_note, draw_secondary,
    form_inner, left_of,
};
use crate::text::truncate_left;
use crate::ui::View;
use crate::ui::widgets::{hovered, overlay_block};

const FORM_HEIGHT: u16 = 10;

pub fn form_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(FORM_WIDTH);
    let height = FORM_HEIGHT.min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub(in crate::ui) fn form_rows(form: Rect) -> [Rect; 6] {
    let [label, input, hint, toggle, note, _, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(form_inner(form));
    [label, input, hint, toggle, note, buttons]
}

pub(crate) fn form_toggle(form: Rect) -> Rect {
    form_rows(form)[3]
}

pub fn form_buttons(form: Rect, submit: &str) -> [Rect; 2] {
    buttons_in(form_rows(form)[5], submit, CANCEL_LABEL)
}

pub(crate) fn form_extra_button(form: Rect, submit: &str, extra: &str) -> Rect {
    let [submit, _] = form_buttons(form, submit);
    left_of(form_rows(form)[5], submit, extra)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormHit {
    Submit,
    Extra,
    Cancel,
    Toggle,
}

pub(crate) fn form_hit(area: Rect, submit: &str, extra: Option<&str>, pos: Position) -> Option<FormHit> {
    let form = form_area(area);
    let [s, c] = form_buttons(form, submit);
    if s.contains(pos) {
        Some(FormHit::Submit)
    } else if extra.is_some_and(|extra| form_extra_button(form, submit, extra).contains(pos)) {
        Some(FormHit::Extra)
    } else if c.contains(pos) {
        Some(FormHit::Cancel)
    } else if form_toggle(form).contains(pos) {
        Some(FormHit::Toggle)
    } else {
        None
    }
}

pub(crate) struct Toggle {
    pub(crate) label: &'static str,
    pub(crate) on: bool,
}

pub(crate) struct Form {
    pub(crate) title: &'static str,
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) hint: String,
    pub(crate) toggle: Option<Toggle>,
    pub(crate) note: Option<Note>,
    pub(crate) submit: &'static str,
}

pub(crate) struct Confirm {
    pub(crate) title: &'static str,
    pub(crate) message: String,
    pub(crate) note: Option<Note>,
    pub(crate) submit: &'static str,
    pub(crate) extra: Option<&'static str>,
}

pub(in crate::ui) fn draw_form(f: &mut Frame, view: &View, form: &Form) {
    let r = form_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, form.title), r);
    let [label, input, hint, toggle, note, _] = form_rows(r);
    let dim = Style::default().fg(view.muted);

    f.render_widget(Paragraph::new(Span::styled(form.label, dim)), label);
    if let Some(t) = &form.toggle {
        let mark = if t.on { "[x] " } else { "[ ] " };
        let style = if hovered(view, toggle) { Style::default().fg(Color::Cyan) } else { Style::default() };
        f.render_widget(Paragraph::new(Span::styled(format!("{mark}{}", t.label), style)), toggle);
    }
    let max = usize::from(input.width).saturating_sub(INPUT_PROMPT.chars().count() + 1);
    let value = truncate_left(&form.value, max);
    let cursor_x = input.x + u16::try_from(INPUT_PROMPT.chars().count() + value.chars().count()).unwrap_or(0);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(INPUT_PROMPT, Style::default().fg(Color::Cyan)),
            Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        input,
    );
    f.render_widget(Paragraph::new(Span::styled(truncate_left(&form.hint, usize::from(hint.width)), dim)), hint);

    draw_note(f, view.muted, form.note.as_ref(), note);

    draw_dialog_buttons(f, view, form_buttons(r, form.submit), form.submit, CANCEL_LABEL);

    if !matches!(form.note, Some(Note::Busy(_))) && cursor_x < input.right() {
        f.set_cursor_position(Position::new(cursor_x, input.y));
    }
}

pub(in crate::ui) fn draw_confirm(f: &mut Frame, view: &View, confirm: &Confirm) {
    let r = form_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, confirm.title), r);
    let [label, _, _, toggle, note, _] = form_rows(r);
    let message = Rect::new(label.x, label.y, label.width, toggle.bottom().saturating_sub(label.y));
    f.render_widget(Paragraph::new(confirm.message.as_str()).wrap(Wrap { trim: true }), message);
    draw_note(f, view.muted, confirm.note.as_ref(), note);
    draw_dialog_buttons(f, view, form_buttons(r, confirm.submit), confirm.submit, CANCEL_LABEL);
    if let Some(extra) = confirm.extra {
        draw_secondary(f, view, form_extra_button(r, confirm.submit, extra), extra);
    }
}
