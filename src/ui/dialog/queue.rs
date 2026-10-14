use ratatui::Frame;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{buttons_in, draw_dialog_buttons};
use crate::queue::form::{Control, Form, When};
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::overlay_block;

const LABEL: u16 = 9;
const HEIGHT: u16 = 19;
const COMBO: u16 = 18;
const WIDE: u16 = 44;
const CLOCK: u16 = 7;
const DATE: u16 = 12;
const AMOUNT: u16 = 6;
const UNIT: u16 = 12;

pub(crate) enum Hit {
    Control(Control, usize, usize),
    Choose(usize),
    Close,
    Submit,
    Cancel,
}

pub(crate) fn area(screen: Rect) -> Rect {
    let width = screen.width.saturating_sub(2).min(72);
    let height = screen.height.min(HEIGHT);
    Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height)
}

pub(crate) fn area_with_list(screen: Rect, form: &Form) -> Rect {
    list(screen, form).map_or_else(|| area(screen), |list| area(screen).union(list))
}

fn inner(screen: Rect) -> Rect {
    area(screen).inner(Margin::new(2, 1))
}

fn row(screen: Rect, at: u16, height: u16) -> Rect {
    let r = inner(screen);
    Rect::new(r.x, r.y.saturating_add(at), r.width, height).intersection(r)
}

fn cell(screen: Rect, at: u16, x: u16, width: u16) -> Rect {
    let r = row(screen, at, 1);
    Rect::new(r.x.saturating_add(x), r.y, width, r.height).intersection(r)
}

fn rest(screen: Rect, x: u16) -> u16 {
    inner(screen).width.saturating_sub(x)
}

pub(crate) fn rect(screen: Rect, form: &Form, control: Control) -> Rect {
    let on = form.kind() == When::On;
    match control {
        Control::Prompt => row(screen, 1, 3),
        Control::When => cell(screen, 5, LABEL, COMBO),
        Control::Date => cell(screen, 6, LABEL, DATE),
        Control::Clock => cell(screen, 6, if on { LABEL + DATE + 1 } else { LABEL }, CLOCK),
        Control::Amount => cell(screen, 6, LABEL, AMOUNT),
        Control::Unit => cell(screen, 6, LABEL + AMOUNT + 1, UNIT),
        Control::Place => cell(screen, 7, LABEL, rest(screen, LABEL).min(WIDE)),
        Control::OpenIn => cell(screen, 8, LABEL, COMBO),
        Control::Branch => cell(screen, 8, LABEL + COMBO + 1, rest(screen, LABEL + COMBO + 1).min(WIDE - COMBO - 1)),
        Control::Agent => cell(screen, 9, LABEL, COMBO),
        Control::Mode => cell(screen, 10, LABEL, COMBO),
    }
}

pub(crate) fn text(screen: Rect, form: &Form, control: Control) -> Rect {
    let r = rect(screen, form, control);
    if control == Control::Prompt {
        r
    } else {
        Rect { x: r.x.saturating_add(1), width: r.width.saturating_sub(1), ..r }
    }
}

pub(crate) fn list(screen: Rect, form: &Form) -> Option<Rect> {
    let open = form.opened()?;
    Some(combo::list(rect(screen, form, open), form.combo_of(open)?, screen))
}

fn buttons(screen: Rect) -> [Rect; 2] {
    let r = inner(screen);
    buttons_in(Rect { y: r.bottom().saturating_sub(1), height: 1, ..r }, "schedule", "cancel")
}

fn note(screen: Rect) -> Rect {
    let r = inner(screen);
    let buttons = r.bottom().saturating_sub(1);
    let y = buttons.saturating_sub(2).max(r.y.saturating_add(12)).min(buttons);
    Rect::new(r.x, y, r.width, buttons - y)
}

pub(crate) fn first_line(editor: &Editor, field: Rect) -> usize {
    let (line, _) = editor.position(usize::from(field.width.max(1)));
    line.saturating_sub(usize::from(field.height).saturating_sub(1))
}

pub(crate) fn hit(screen: Rect, form: &Form, pos: Position) -> Option<Hit> {
    if let (Some(open), Some(list)) = (form.opened(), list(screen, form)) {
        let choice = form.combo_of(open).and_then(|combo| combo::hit(list, combo, pos));
        return Some(choice.map_or(Hit::Close, Hit::Choose));
    }
    let [submit, cancel] = buttons(screen);
    if submit.contains(pos) {
        return Some(Hit::Submit);
    }
    if cancel.contains(pos) {
        return Some(Hit::Cancel);
    }
    form.controls().into_iter().find_map(|control| {
        let t = text(screen, form, control);
        rect(screen, form, control).contains(pos).then(|| {
            Hit::Control(control, usize::from(pos.y.saturating_sub(t.y)), usize::from(pos.x.saturating_sub(t.x)))
        })
    })
}

fn draw_editor(f: &mut Frame, field: Rect, r: Rect, editor: &Editor, focused: bool, placeholder: &str, view: &View) {
    let bg = Style::default().bg(view.surface());
    f.render_widget(Paragraph::new("").style(bg), field);
    let width = usize::from(r.width.max(1));
    let (line, col) = editor.position(width);
    let start = first_line(editor, r);
    if editor.is_empty() && !placeholder.is_empty() {
        f.render_widget(Paragraph::new(placeholder).style(bg.fg(view.muted)), r);
    } else {
        let chars: Vec<_> = editor
            .text()
            .chars()
            .map(|c| match c {
                '\n' => '↵',
                c if c.is_control() => ' ',
                c => c,
            })
            .collect();
        let lines: Vec<_> =
            editor.lines(width).into_iter().skip(start).map(|range| chars[range].iter().collect::<String>()).collect();
        let text = bg.fg(if view.light { Color::Black } else { Color::White }).add_modifier(Modifier::BOLD);
        f.render_widget(Paragraph::new(lines.join("\n")).style(text), r);
    }
    if focused && !r.is_empty() {
        f.set_cursor_position(Position::new(
            r.x + u16::try_from(col).unwrap_or_default().min(r.width.saturating_sub(1)),
            r.y + u16::try_from(line - start).unwrap_or_default().min(r.height.saturating_sub(1)),
        ));
    }
}

fn hint(form: &Form) -> &'static str {
    match form.kind() {
        When::At => "today or tomorrow",
        When::Tomorrow | When::On | When::In => "",
        When::ClaudeReset | When::CodexReset => "read from the plan when you save",
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let area = area(screen);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, "Schedule an agent"), area);
    let dim = Style::default().fg(view.muted);
    let controls = form.controls();
    for (at, label) in [(0, "Prompt"), (5, "When"), (7, "Where"), (8, "Open in"), (9, "Agent"), (10, "Mode")] {
        if at != 8 || controls.contains(&Control::OpenIn) {
            f.render_widget(Paragraph::new(Span::styled(label, dim)), cell(screen, at, 0, LABEL));
        }
    }
    let typing = !form.busy && form.opened().is_none();
    let placeholder = if form.places.is_empty() { "no open project" } else { "choose an open place…" };
    for control in &controls {
        let r = rect(screen, form, *control);
        let focused = typing && form.focus == *control;
        if let Some(editor) = form.editor_of(*control) {
            let placeholder = if *control == Control::Branch { "branch name" } else { "" };
            draw_editor(f, r, text(screen, form, *control), editor, focused, placeholder, view);
        } else if let Some(choices) = form.combo_of(*control) {
            let empty = if *control == Control::Place { placeholder } else { "" };
            combo::draw_field(f, view, r, choices, empty, form.focus == *control && !form.busy);
        }
    }
    let values =
        controls.iter().filter(|c| matches!(c, Control::Clock | Control::Date | Control::Amount | Control::Unit));
    let end = values.map(|c| rect(screen, form, *c).right()).max().unwrap_or(inner(screen).x + LABEL);
    let r = row(screen, 6, 1);
    let tip = Rect::new(end.saturating_add(1), r.y, r.right().saturating_sub(end.saturating_add(1)), r.height);
    f.render_widget(Paragraph::new(Span::styled(hint(form), dim)), tip);
    let notice = format!("Times use {}. Queued agents always accept their folder trust question.", form.zone);
    f.render_widget(Paragraph::new(Span::styled(notice, dim)).wrap(Wrap { trim: true }), row(screen, 12, 2));
    let note = if form.busy { Some("reading the plan reset…".to_string()) } else { form.error.clone() };
    if let Some(note) = note {
        let r = self::note(screen);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(note)
                .style(Style::default().fg(if form.busy { view.muted } else { Color::Red }))
                .wrap(Wrap { trim: true }),
            r,
        );
    }
    draw_dialog_buttons(f, view, buttons(screen), "schedule", "cancel");
    if let (Some(open), Some(list)) = (form.opened(), list(screen, form))
        && let Some(choices) = form.combo_of(open)
    {
        combo::draw_list(f, view, list, choices);
    }
}
