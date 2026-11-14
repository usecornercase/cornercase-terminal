use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{bottom_buttons, centred_in, draw_dialog_buttons, inset, padded, row_in, with_list};
use crate::bots::form::{Field, Form};
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::{EditorField, draw_editor, overlay_block};

const LABEL: u16 = 9;
const HEIGHT: u16 = 19;
const WIDE: u16 = 46;
const ABOUT_ROWS: u16 = 4;
const CORNERCASE_NOTE: &str = "through cornercase: reads anything, writes only in its folder";
const FULL_NOTE: &str = "full access: can change any file and run anything, unattended";

pub(crate) enum Hit {
    Field(Field, usize, usize),
    Choose(usize),
    Close,
    Submit,
    Cancel,
}

pub(crate) fn area(screen: Rect) -> Rect {
    centred_in(screen, 76, HEIGHT)
}

pub(crate) fn area_with_list(screen: Rect, form: &Form) -> Rect {
    with_list(area(screen), list(screen, form))
}

fn inner(screen: Rect) -> Rect {
    padded(area(screen))
}

fn row_of(form: &Form, field: Field) -> u16 {
    let at = form.fields().iter().position(|shown| *shown == field).unwrap_or_default();
    let notes = if field == Field::About { 2 } else { 0 };
    u16::try_from(at).unwrap_or_default() + notes
}

fn row(screen: Rect, at: u16, height: u16) -> Rect {
    row_in(inner(screen), at, height)
}

pub(crate) fn rect(screen: Rect, form: &Form, field: Field) -> Rect {
    let r = inner(screen);
    let width = r.width.saturating_sub(LABEL);
    let at = row_of(form, field);
    if field == Field::About {
        let top = row(screen, at, ABOUT_ROWS);
        return Rect { x: top.x + LABEL, width, ..top }.intersection(top);
    }
    let cell = row(screen, at, 1);
    Rect { x: cell.x + LABEL, width: width.min(WIDE), ..cell }.intersection(cell)
}

pub(crate) fn text(screen: Rect, form: &Form, field: Field) -> Rect {
    let r = rect(screen, form, field);
    inset(r)
}

pub(crate) fn list(screen: Rect, form: &Form) -> Option<Rect> {
    let open = form.opened()?;
    Some(combo::list(rect(screen, form, open), form.combo_of(open)?, screen))
}

pub(crate) fn submit_label(form: &Form) -> &'static str {
    if form.editing.is_some() { "save" } else { "create" }
}

fn buttons(screen: Rect, form: &Form) -> [Rect; 2] {
    bottom_buttons(inner(screen), submit_label(form))
}

fn error_row(screen: Rect) -> Rect {
    let r = inner(screen);
    Rect { y: r.bottom().saturating_sub(3), height: 2.min(r.height), ..r }
}

pub(crate) fn hit(screen: Rect, form: &Form, pos: Position) -> Option<Hit> {
    let open = form.opened().and_then(|open| form.combo_of(open));
    match combo::open_hit(list(screen, form), open, pos) {
        Some(combo::Choice::Item(i)) => return Some(Hit::Choose(i)),
        Some(combo::Choice::Outside) => return Some(Hit::Close),
        None => {}
    }
    let [submit, cancel] = buttons(screen, form);
    if submit.contains(pos) {
        return Some(Hit::Submit);
    }
    if cancel.contains(pos) {
        return Some(Hit::Cancel);
    }
    form.fields().into_iter().find_map(|field| {
        let t = text(screen, form, field);
        rect(screen, form, field)
            .contains(pos)
            .then(|| Hit::Field(field, usize::from(pos.y.saturating_sub(t.y)), usize::from(pos.x.saturating_sub(t.x))))
    })
}

fn draw_labels(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let dim = Style::default().fg(view.muted);
    let labels = [
        (Field::Name, "Name"),
        (Field::Agent, "Agent"),
        (Field::Model, "Model"),
        (Field::Effort, "Effort"),
        (Field::Account, "Account"),
        (Field::Access, "Access"),
        (Field::About, "For"),
    ];
    for (field, label) in labels {
        if form.fields().contains(&field) {
            let r = row(screen, row_of(form, field), 1);
            f.render_widget(Paragraph::new(Span::styled(label, dim)), Rect { width: LABEL, ..r });
        }
    }
}

fn draw_fields(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let typing = form.opened().is_none();
    for field in form.fields() {
        let r = rect(screen, form, field);
        let focused = typing && form.focus == field;
        if let Some(editor) = form.editor_of(field) {
            let placeholder = if field == Field::About { "optional: what the bot is for" } else { "" };
            let at = EditorField { field: r, text: text(screen, form, field), focused, placeholder };
            draw_editor(f, view, editor, at);
        } else if let Some(choices) = form.combo_of(field) {
            let grey = field == Field::Agent && form.editing.is_some();
            draw_combo(f, view, r, (choices, field, focused), grey);
        }
    }
}

fn draw_combo(
    f: &mut Frame,
    view: &View,
    r: Rect,
    (choices, field, focused): (&crate::list::Combo, Field, bool),
    grey: bool,
) {
    if grey {
        let label = choices.label().unwrap_or_default();
        f.render_widget(
            Paragraph::new(Span::styled(label.to_string(), Style::default().fg(view.muted))),
            Rect { x: r.x + 1, ..r },
        );
        return;
    }
    let placeholder = if field == Field::Access { "choose its access" } else { "" };
    combo::draw_field(f, view, r, combo::Field { combo: choices, placeholder, focused });
}

fn draw_notes(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let access = row(screen, row_of(form, Field::Access) + 1, 1);
    let x = access.x + LABEL + 1;
    let width = usize::from(access.width.saturating_sub(LABEL + 1));
    for (i, (line, colour)) in [(CORNERCASE_NOTE, view.muted), (FULL_NOTE, Color::Red)].into_iter().enumerate() {
        let y = access.y + u16::try_from(i).unwrap_or_default();
        let r = Rect::new(x, y, access.width.saturating_sub(LABEL + 1), 1);
        f.render_widget(Paragraph::new(Span::styled(truncate_right(line, width), Style::default().fg(colour))), r);
    }
}

fn draw_error(f: &mut Frame, screen: Rect, form: &Form) {
    if let Some(error) = &form.error {
        let r = error_row(screen);
        f.render_widget(
            Paragraph::new(error.clone()).style(Style::default().fg(Color::Red)).wrap(Wrap { trim: true }),
            r,
        );
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let area = area(screen);
    let title = form.editing.as_ref().map_or_else(|| String::from("New bot"), |id| format!("Edit bot · {id}"));
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, &title), area);
    draw_labels(f, view, screen, form);
    draw_fields(f, view, screen, form);
    draw_notes(f, view, screen, form);
    draw_error(f, screen, form);
    draw_dialog_buttons(f, view, buttons(screen, form), submit_label(form), "cancel");
    let open = form.opened().and_then(|open| form.combo_of(open));
    combo::draw_open(f, view, list(screen, form), open);
}
