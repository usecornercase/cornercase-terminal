use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{bottom_buttons, centred_in, dialog_button_style, draw_dialog_buttons, inset, padded, with_list};
use crate::bots::form::{Field, Form};
use crate::editor::wrap_text;
use crate::text::truncate_right;
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::{EditorField, draw_editor, overlay_block};

const LABEL: u16 = 12;
const WIDTH: u16 = 76;
const ABOUT_ROWS: u16 = 4;
const TIGHT_ABOUT_ROWS: u16 = 3;
const CHROME: u16 = 4;
const NOTE_INDENT: u16 = 1;
const CORNERCASE_NOTE: &str = "reads anything, writes only in its folder";
const FOLDER_NOTE: &str = "reads and writes only in its folder";
const FULL_NOTE: &str = "can change any file and run anything, unattended";
const NO_ACCESS_NOTE: &str = "choose how far it may reach";
const UNPAIRED: &str = "pair Telegram or Discord in Settings › Remote";
const FOLLOW: &str = "tell it when an agent it starts finishes";
const KEPT_AGENT: &str = "a bot keeps its agent";
const MANAGE: &str = "manage…";
const NEW_ABOUT: &str = "optional: what it does. It sets itself up from this.";
const EDIT_ABOUT: &str = "what the bot is for";

pub(crate) enum Hit {
    Field(Field, usize, usize),
    Choose(usize),
    Close,
    Submit,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Air {
    Tight,
    Grouped,
    Airy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Field(Field),
    Note,
    Gap,
}

fn group(field: Field) -> u8 {
    match field {
        Field::Name => 0,
        Field::About => 1,
        Field::Agent | Field::Model | Field::Effort | Field::Account | Field::Access => 2,
        Field::Follow | Field::ChatApps | Field::Secrets => 3,
    }
}

fn is_bar(field: Field) -> bool {
    !matches!(field, Field::Follow | Field::ChatApps | Field::Secrets)
}

fn gap_before(previous: Field, field: Field, air: Air) -> bool {
    match air {
        Air::Airy => group(previous) != group(field) || (is_bar(previous) && is_bar(field)),
        Air::Grouped => group(previous) != group(field),
        Air::Tight => false,
    }
}

fn plan(form: &Form, air: Air) -> Vec<(Slot, u16)> {
    let mut slots = Vec::new();
    let mut previous = None;
    for field in form.fields() {
        if previous.is_some_and(|previous| gap_before(previous, field, air)) {
            slots.push((Slot::Gap, 1));
        }
        let height = match field {
            Field::About if air == Air::Tight => TIGHT_ABOUT_ROWS,
            Field::About => ABOUT_ROWS,
            _ => 1,
        };
        slots.push((Slot::Field(field), height));
        if field == Field::Access {
            slots.push((Slot::Note, 1));
        }
        previous = Some(field);
    }
    slots
}

fn margin(air: Air) -> u16 {
    u16::from(air != Air::Tight)
}

fn height_of(air: Air, slots: &[(Slot, u16)]) -> u16 {
    slots.iter().map(|(_, height)| height).sum::<u16>() + CHROME + 2 * margin(air)
}

fn fitted(screen: Rect, form: &Form) -> (Air, Vec<(Slot, u16)>) {
    let room = screen.height.saturating_sub(2);
    [Air::Airy, Air::Grouped]
        .into_iter()
        .map(|air| (air, plan(form, air)))
        .find(|(air, slots)| height_of(*air, slots) <= room)
        .unwrap_or_else(|| (Air::Tight, plan(form, Air::Tight)))
}

pub(crate) fn area(screen: Rect, form: &Form) -> Rect {
    let (air, slots) = fitted(screen, form);
    centred_in(screen, WIDTH, height_of(air, &slots))
}

pub(crate) fn area_with_list(screen: Rect, form: &Form) -> Rect {
    with_list(area(screen, form), list(screen, form))
}

fn inner(screen: Rect, form: &Form) -> Rect {
    let (air, _) = fitted(screen, form);
    let r = padded(area(screen, form));
    let margin = margin(air).min(r.height / 2);
    Rect { y: r.y + margin, height: r.height - 2 * margin, ..r }
}

fn slot_row(screen: Rect, form: &Form, wanted: Slot) -> Rect {
    let r = inner(screen, form);
    let mut y = r.y;
    for (slot, height) in fitted(screen, form).1 {
        if slot == wanted {
            return Rect::new(r.x, y, r.width, height).intersection(r);
        }
        y = y.saturating_add(height);
    }
    Rect::default()
}

pub(crate) fn rect(screen: Rect, form: &Form, field: Field) -> Rect {
    let row = slot_row(screen, form, Slot::Field(field));
    Rect { x: row.x + LABEL.min(row.width), width: row.width.saturating_sub(LABEL), ..row }
}

pub(crate) fn text(screen: Rect, form: &Form, field: Field) -> Rect {
    let r = inset(rect(screen, form, field));
    if field == Field::About { Rect { width: r.width.saturating_sub(1), ..r } } else { r }
}

pub(crate) fn list(screen: Rect, form: &Form) -> Option<Rect> {
    let open = form.opened()?;
    form.combo_of(open).map(|choices| combo::list(rect(screen, form, open), choices, screen))
}

pub(crate) fn submit_label(form: &Form) -> &'static str {
    if form.editing.is_some() { "save" } else { "create" }
}

fn buttons(screen: Rect, form: &Form) -> [Rect; 2] {
    bottom_buttons(inner(screen, form), submit_label(form))
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

fn label_of(field: Field) -> &'static str {
    match field {
        Field::Name => "Name",
        Field::About => "For",
        Field::Agent => "Agent",
        Field::Model => "Model",
        Field::Effort => "Effort",
        Field::Account => "Account",
        Field::Access => "Access",
        Field::Follow => "Follow",
        Field::ChatApps => "Chat apps",
        Field::Secrets => "Secrets",
    }
}

fn draw_labels(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let typing = form.opened().is_none();
    for field in form.fields() {
        let row = slot_row(screen, form, Slot::Field(field));
        let style = if form.focus == field && (typing || form.opened() == Some(field)) {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(view.muted)
        };
        f.render_widget(Paragraph::new(Span::styled(label_of(field), style)), Rect { width: LABEL, height: 1, ..row });
    }
}

fn draw_fields(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let typing = form.opened().is_none();
    for field in form.fields() {
        let r = rect(screen, form, field);
        let focused = typing && form.focus == field;
        match field {
            Field::Follow => draw_follow(f, view, r, (form.follow, focused)),
            Field::Secrets => draw_secrets(f, view, r, (&form.secrets, focused)),
            Field::ChatApps if !form.chat_ready => draw_aside(f, r, UNPAIRED, Style::default().fg(view.muted)),
            Field::Agent if form.editing.is_some() => draw_kept_agent(f, view, r, form),
            _ => draw_input(f, view, (screen, r), form, (field, focused)),
        }
    }
}

fn draw_input(f: &mut Frame, view: &View, (screen, r): (Rect, Rect), form: &Form, (field, focused): (Field, bool)) {
    if let Some(editor) = form.editor_of(field) {
        let placeholder = match field {
            Field::About if form.editing.is_some() => EDIT_ABOUT,
            Field::About => NEW_ABOUT,
            _ => "",
        };
        let at = EditorField { field: r, text: text(screen, form, field), focused, placeholder, top: None };
        draw_editor(f, view, editor, at);
    } else if let Some(choices) = form.combo_of(field) {
        let placeholder = if field == Field::Access { "choose its access" } else { "" };
        combo::draw_field(f, view, r, combo::Field { combo: choices, placeholder, focused });
    }
}

fn draw_aside(f: &mut Frame, r: Rect, text: &str, style: Style) {
    let at = inset(r);
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(at.width)), style)), at);
}

fn draw_kept_agent(f: &mut Frame, view: &View, r: Rect, form: &Form) {
    let label = form.agent.label().unwrap_or_default().to_string();
    let line = Line::from(vec![
        Span::styled(label, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  · {KEPT_AGENT}"), Style::default().fg(view.muted)),
    ]);
    f.render_widget(Paragraph::new(line), inset(r));
}

fn draw_follow(f: &mut Frame, view: &View, r: Rect, (follow, focused): (bool, bool)) {
    let text = if focused { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() };
    let mark = if follow {
        Span::styled("x", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::raw(" ")
    };
    let bracket = Style::default().fg(if focused { Color::Cyan } else { view.muted });
    let line = Line::from(vec![
        Span::styled("[", bracket),
        mark,
        Span::styled("]", bracket),
        Span::raw(" "),
        Span::styled(FOLLOW, text),
    ]);
    f.render_widget(Paragraph::new(line), inset(r));
}

fn draw_secrets(f: &mut Frame, view: &View, r: Rect, (summary, focused): (&str, bool)) {
    let width = u16::try_from(MANAGE.chars().count() + 2).unwrap_or_default().min(r.width);
    let button = Rect { x: r.right().saturating_sub(width), width, ..r };
    let room = usize::from(button.x.saturating_sub(r.x + 2));
    let style = if focused { Style::default().add_modifier(Modifier::BOLD) } else { Style::default() };
    draw_aside(f, r, &truncate_right(summary, room), style);
    let style = dialog_button_style(view, button, focused);
    f.render_widget(Paragraph::new(Span::styled(format!(" {MANAGE} "), style)), button);
}

fn draw_note(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let row = slot_row(screen, form, Slot::Note);
    let at = Rect { x: row.x + LABEL + NOTE_INDENT, width: row.width.saturating_sub(LABEL + NOTE_INDENT), ..row };
    let (note, colour) = match form.access.chosen() {
        None => (NO_ACCESS_NOTE, view.muted),
        Some(0) if form.reads_only_folder() => (FOLDER_NOTE, view.muted),
        Some(0) => (CORNERCASE_NOTE, view.muted),
        Some(_) => (FULL_NOTE, Color::Red),
    };
    let line = truncate_right(note, usize::from(at.width));
    f.render_widget(Paragraph::new(Span::styled(line, Style::default().fg(colour))), at);
}

fn draw_error(f: &mut Frame, screen: Rect, form: &Form) {
    let Some(error) = &form.error else { return };
    let [submit, _] = buttons(screen, form);
    let r = inner(screen, form);
    let width = submit.x.saturating_sub(r.x + 2);
    let lines = wrap_text(error, usize::from(width).max(1));
    let shown = &lines[lines.len().saturating_sub(2)..];
    let red = Style::default().fg(Color::Red);
    for (i, line) in shown.iter().rev().enumerate() {
        let y = submit.y.saturating_sub(u16::try_from(i).unwrap_or_default());
        let at = Rect { y, width, height: 1, ..r }.intersection(r);
        f.render_widget(Paragraph::new(Span::styled(line.clone(), red)), at);
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let area = area(screen, form);
    let title = form.editing.as_ref().map_or_else(|| String::from("New bot"), |id| format!("Edit bot · {id}"));
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, &title), area);
    draw_labels(f, view, screen, form);
    draw_fields(f, view, screen, form);
    draw_note(f, view, screen, form);
    draw_error(f, screen, form);
    draw_dialog_buttons(f, view, buttons(screen, form), submit_label(form), "cancel");
    let open = form.opened().and_then(|open| form.combo_of(open));
    combo::draw_open(f, view, list(screen, form), open);
}
