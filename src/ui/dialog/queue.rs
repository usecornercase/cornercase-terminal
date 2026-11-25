use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{bottom_buttons, centred_in, draw_dialog_buttons, inset, padded, row_in, with_list};
use crate::queue::form::{Control, Form, When};
use crate::ui::View;
use crate::ui::combo;
use crate::ui::widgets::overlay_block;
use crate::ui::widgets::{EditorField, draw_editor};

const LABEL: u16 = 9;
const HEIGHT: u16 = 23;
const PROMPT: u16 = 3;
const PROMPT_MOST: u16 = 8;
const PROMPT_HINT: &str = "enter breaks the line · tab moves on";
const WHERE: u16 = 8;
const OPEN_IN: u16 = 10;
const AGENT: u16 = 12;
const ACCOUNT: u16 = LABEL + COMBO + 2;
const MODE: u16 = 14;
const NOTICE: u16 = 16;
const CLEANUP: u16 = 16;
const ROUTINE_NOTICE: u16 = 18;
const DAY: u16 = 12;
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

fn extra(screen: Rect) -> u16 {
    screen.height.saturating_sub(2).saturating_sub(HEIGHT).min(PROMPT_MOST - PROMPT)
}

fn at(screen: Rect, row: u16) -> u16 {
    if row > PROMPT { row + extra(screen) } else { row }
}

pub(crate) fn area(screen: Rect) -> Rect {
    centred_in(screen, 72, HEIGHT + extra(screen))
}

pub(crate) fn area_with_list(screen: Rect, form: &Form) -> Rect {
    with_list(area(screen), list(screen, form))
}

fn inner(screen: Rect) -> Rect {
    padded(area(screen))
}

fn row(screen: Rect, row: u16, height: u16) -> Rect {
    row_in(inner(screen), at(screen, row), height)
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
        Control::Prompt => row(screen, 1, PROMPT + extra(screen)),
        Control::When => cell(screen, 5, LABEL, COMBO),
        Control::Date => cell(screen, 6, LABEL, DATE),
        Control::Day => cell(screen, 6, LABEL, DAY),
        Control::Clock => cell(screen, 6, clock_at(form.kind(), on), CLOCK),
        Control::Amount => cell(screen, 6, LABEL, AMOUNT),
        Control::Unit => cell(screen, 6, LABEL + AMOUNT + 1, UNIT),
        Control::Place => cell(screen, WHERE, LABEL, rest(screen, LABEL).min(WIDE)),
        Control::OpenIn => cell(screen, OPEN_IN, LABEL, COMBO),
        Control::Branch => {
            cell(screen, OPEN_IN, LABEL + COMBO + 1, rest(screen, LABEL + COMBO + 1).min(WIDE - COMBO - 1))
        }
        Control::Agent => cell(screen, AGENT, LABEL, COMBO),
        Control::Account => cell(screen, AGENT, ACCOUNT + LABEL, COMBO),
        Control::Mode => cell(screen, MODE, LABEL, COMBO),
        Control::Cleanup => cell(screen, CLEANUP, LABEL, rest(screen, LABEL)),
    }
}

fn clock_at(kind: When, on: bool) -> u16 {
    match kind {
        When::Weekly => LABEL + DAY + 1,
        _ if on => LABEL + DATE + 1,
        _ => LABEL,
    }
}

pub(crate) fn text(screen: Rect, form: &Form, control: Control) -> Rect {
    let r = rect(screen, form, control);
    if control == Control::Prompt { r } else { inset(r) }
}

pub(crate) fn top(screen: Rect, form: &Form) -> usize {
    let r = text(screen, form, Control::Prompt);
    let lines = form.prompt.lines(usize::from(r.width.max(1))).len();
    form.prompt.scroll().min(lines.saturating_sub(usize::from(r.height.max(1))))
}

pub(crate) fn list(screen: Rect, form: &Form) -> Option<Rect> {
    let open = form.opened()?;
    Some(combo::list(rect(screen, form, open), form.combo_of(open)?, screen))
}

fn buttons(screen: Rect) -> [Rect; 2] {
    bottom_buttons(inner(screen), "schedule")
}

#[cfg(test)]
pub(crate) fn schedule_button(screen: Rect) -> Rect {
    buttons(screen)[0]
}

fn note(screen: Rect) -> Rect {
    let r = inner(screen);
    let buttons = r.bottom().saturating_sub(1);
    let y = buttons.saturating_sub(2).max(r.y.saturating_add(at(screen, NOTICE))).min(buttons);
    Rect::new(r.x, y, r.width, buttons - y)
}

pub(crate) fn hit(screen: Rect, form: &Form, pos: Position) -> Option<Hit> {
    let open = form.opened().and_then(|open| form.combo_of(open));
    match combo::open_hit(list(screen, form), open, pos) {
        Some(combo::Choice::Item(i)) => return Some(Hit::Choose(i)),
        Some(combo::Choice::Outside) => return Some(Hit::Close),
        None => {}
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

fn hint(form: &Form) -> &'static str {
    match form.kind() {
        When::At => "today or tomorrow",
        When::Every => "at least 30 minutes apart",
        When::Tomorrow | When::On | When::In | When::Daily | When::Weekdays | When::Weekly => "",
        When::ClaudeReset | When::CodexReset => "read from the plan when you save",
        When::Free => "starts on the machine with room, in turn",
    }
}

fn draw_labels(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let dim = Style::default().fg(view.muted);
    let controls = form.controls();
    if form.bot().is_some() {
        for (at, label) in [(0, "Prompt"), (5, "When"), (WHERE, "Where"), (AGENT, "Agent")] {
            f.render_widget(Paragraph::new(Span::styled(label, dim)), cell(screen, at, 0, LABEL));
        }
        let set = cell(screen, AGENT, LABEL + 1, rest(screen, LABEL + 1));
        f.render_widget(Paragraph::new(Span::styled("set by the bot", dim)), set);
        if let Some(choice) = form.choice() {
            let at = cell(screen, WHERE, LABEL + 1, rest(screen, LABEL + 1));
            f.render_widget(Paragraph::new(Span::styled(choice.label.as_str(), dim)), at);
        }
        return;
    }
    for (at, label) in
        [(0, "Prompt"), (5, "When"), (WHERE, "Where"), (OPEN_IN, "Open in"), (AGENT, "Agent"), (MODE, "Mode")]
    {
        f.render_widget(Paragraph::new(Span::styled(label, dim)), cell(screen, at, 0, LABEL));
    }
    if controls.contains(&Control::Account) {
        f.render_widget(Paragraph::new(Span::styled("Account", dim)), cell(screen, AGENT, ACCOUNT, LABEL));
    }
    if !controls.contains(&Control::OpenIn) {
        let only = cell(screen, OPEN_IN, LABEL + 1, rest(screen, LABEL + 1));
        let text = crate::queue::form::OPEN_IN[usize::from(form.kind().repeats())];
        f.render_widget(Paragraph::new(Span::styled(text, dim)), only);
    }
    if form.flow().is_some() {
        let flow = cell(screen, MODE, LABEL + 1, rest(screen, LABEL + 1));
        f.render_widget(Paragraph::new(Span::styled("set by the flow's roles", dim)), flow);
    }
}

fn draw_cleanup(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    if !form.kind().repeats() || form.bot().is_some() {
        return;
    }
    let dim = Style::default().fg(view.muted);
    let mark = if form.cleanup && form.cleanup_off.is_none() { "[x]" } else { "[ ]" };
    let text = format!("{mark} remove the workspace when it finishes");
    let focused = form.focus == Control::Cleanup && !form.busy;
    let style = match (&form.cleanup_off, focused) {
        (Some(_), _) => dim,
        (None, true) => Style::default().add_modifier(Modifier::BOLD),
        (None, false) => Style::default(),
    };
    f.render_widget(Paragraph::new(Span::styled(text, style)), cell(screen, CLEANUP, LABEL, rest(screen, LABEL)));
    if let Some(reason) = &form.cleanup_off {
        f.render_widget(
            Paragraph::new(Span::styled(reason.clone(), dim)),
            cell(screen, CLEANUP + 1, LABEL + 4, rest(screen, LABEL + 4)),
        );
    }
}

fn draw_controls(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let typing = !form.busy && form.opened().is_none();
    let placeholder = if form.places.is_empty() { "no open project" } else { "choose an open place…" };
    for control in &form.controls() {
        let r = rect(screen, form, *control);
        let focused = typing && form.focus == *control;
        if let Some(editor) = form.editor_of(*control) {
            let placeholder = if *control == Control::Branch { "branch name" } else { "" };
            let top = (*control == Control::Prompt).then(|| top(screen, form));
            let at = EditorField { field: r, text: text(screen, form, *control), focused, placeholder, top };
            draw_editor(f, view, editor, at);
        } else if let Some(choices) = form.combo_of(*control) {
            let gone = form.account.as_ref().and_then(|account| account.missing()).map(|name| format!("{name} (gone)"));
            let empty = match control {
                Control::Place => placeholder,
                Control::Account => gone.as_deref().unwrap_or_default(),
                _ => "",
            };
            let focused = form.focus == *control && !form.busy;
            combo::draw_field(f, view, r, combo::Field { combo: choices, placeholder: empty, focused });
        }
    }
}

fn draw_prompt_marks(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let dim = Style::default().fg(view.muted);
    let field = text(screen, form, Control::Prompt);
    if form.focus == Control::Prompt && !form.busy && form.opened().is_none() {
        let hint = cell(screen, 0, LABEL, rest(screen, LABEL));
        f.render_widget(Paragraph::new(Span::styled(PROMPT_HINT, dim)), hint);
    }
    let lines = form.prompt.lines(usize::from(field.width.max(1))).len();
    let above = top(screen, form);
    let below = lines.saturating_sub(above + usize::from(field.height));
    for (count, arrow, y) in [(above, "↑", 0), (below, "↓", PROMPT + 1)] {
        if count > 0 {
            let mark = combo::more(arrow, count);
            let width = u16::try_from(mark.chars().count()).unwrap_or_default();
            let r = cell(screen, y, rest(screen, 0).saturating_sub(width), width);
            f.render_widget(Clear, r);
            f.render_widget(Paragraph::new(Span::styled(mark, dim)), r);
        }
    }
}

fn draw_notes(f: &mut Frame, view: &View, screen: Rect, form: &Form) {
    let dim = Style::default().fg(view.muted);
    let values = form
        .controls()
        .into_iter()
        .filter(|c| matches!(c, Control::Clock | Control::Date | Control::Amount | Control::Unit | Control::Day));
    let end = values.map(|c| rect(screen, form, c).right()).max().unwrap_or(inner(screen).x + LABEL);
    let r = row(screen, 6, 1);
    let tip = Rect::new(end.saturating_add(1), r.y, r.right().saturating_sub(end.saturating_add(1)), r.height);
    f.render_widget(Paragraph::new(Span::styled(hint(form), dim)), tip);
    let (notice, at) = if form.bot().is_some() {
        let text =
            format!("Times use {}. Each run is a new conversation of the bot, which answers in its chat.", form.zone);
        (text, NOTICE)
    } else if form.kind().repeats() {
        let text = format!(
            "Times use {}. Each run gets a new worktree from the up-to-date default branch and accepts its folder trust question.",
            form.zone
        );
        (text, ROUTINE_NOTICE)
    } else {
        (format!("Times use {}. Queued agents always accept their folder trust question.", form.zone), NOTICE)
    };
    f.render_widget(Paragraph::new(Span::styled(notice, dim)).wrap(Wrap { trim: true }), row(screen, at, 2));
    let note = if form.busy { Some("reading the plan reset…".to_string()) } else { form.error.clone() };
    if let Some(note) = note {
        let r = self::note(screen);
        f.render_widget(Clear, r);
        let style = Style::default().fg(if form.busy { view.muted } else { Color::Red });
        f.render_widget(Paragraph::new(note).style(style).wrap(Wrap { trim: true }), r);
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let area = area(screen);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, "Schedule an agent"), area);
    draw_labels(f, view, screen, form);
    draw_controls(f, view, screen, form);
    draw_prompt_marks(f, view, screen, form);
    draw_cleanup(f, view, screen, form);
    draw_notes(f, view, screen, form);
    draw_dialog_buttons(f, view, buttons(screen), "schedule", "cancel");
    let open = form.opened().and_then(|open| form.combo_of(open));
    combo::draw_open(f, view, list(screen, form), open);
}
