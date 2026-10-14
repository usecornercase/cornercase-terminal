use ratatui::Frame;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{buttons_in, draw_dialog_buttons, draw_secondary};
use crate::queue::form::Field;
use crate::todo::editor::Editor;
use crate::ui::View;
use crate::ui::widgets::{hovered, overlay_block};

pub(crate) struct Form {
    pub(crate) prompt: Editor,
    pub(crate) at: Editor,
    pub(crate) branch: Editor,
    pub(crate) field: Field,
    pub(crate) zone: String,
    pub(crate) place: String,
    pub(crate) agent: String,
    pub(crate) mode: String,
    pub(crate) worktree: bool,
    pub(crate) note: Option<String>,
    pub(crate) busy: bool,
}

pub(crate) enum Hit {
    Field(Field, usize, usize),
    Place,
    Agent,
    Mode,
    Worktree,
    ClaudeReset,
    CodexReset,
    Submit,
    Cancel,
}

pub(crate) fn area(screen: Rect) -> Rect {
    let width = screen.width.saturating_sub(2).min(72);
    let height = screen.height.min(22);
    Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height)
}

fn inner(screen: Rect) -> Rect {
    area(screen).inner(Margin::new(2, 1))
}

fn row(screen: Rect, at: u16, height: u16) -> Rect {
    let r = inner(screen);
    Rect::new(r.x, r.y.saturating_add(at), r.width, height).intersection(r)
}

pub(crate) fn field(screen: Rect, field: Field) -> Rect {
    match field {
        Field::Prompt => row(screen, 1, 3),
        Field::At => row(screen, 6, 1),
        Field::Branch => row(screen, 13, 1),
    }
}

fn buttons(screen: Rect) -> [Rect; 2] {
    let r = inner(screen);
    buttons_in(Rect { y: r.bottom().saturating_sub(1), height: 1, ..r }, "schedule", "cancel")
}

fn reset(screen: Rect) -> [Rect; 2] {
    let r = row(screen, 7, 1);
    [
        Rect::new(r.x, r.y, 14.min(r.width), r.height),
        Rect::new(r.x.saturating_add(15), r.y, r.width.saturating_sub(15).min(13), r.height),
    ]
}

fn note(screen: Rect) -> Rect {
    let r = inner(screen);
    let buttons = r.bottom().saturating_sub(1);
    let y = buttons.saturating_sub(2).max(r.y.saturating_add(15)).min(buttons);
    Rect::new(r.x, y, r.width, buttons - y)
}

pub(crate) fn first_line(editor: &Editor, field: Rect) -> usize {
    let (line, _) = editor.position(usize::from(field.width.max(1)));
    line.saturating_sub(usize::from(field.height).saturating_sub(1))
}

pub(crate) fn hit(screen: Rect, pos: Position) -> Option<Hit> {
    let [submit, cancel] = buttons(screen);
    if submit.contains(pos) {
        return Some(Hit::Submit);
    }
    if cancel.contains(pos) {
        return Some(Hit::Cancel);
    }
    let [claude, codex] = reset(screen);
    if claude.contains(pos) {
        return Some(Hit::ClaudeReset);
    }
    if codex.contains(pos) {
        return Some(Hit::CodexReset);
    }
    for (r, hit) in [
        (row(screen, 9, 1), Hit::Place),
        (row(screen, 10, 1), Hit::Agent),
        (row(screen, 11, 1), Hit::Mode),
        (row(screen, 12, 1), Hit::Worktree),
    ] {
        if r.contains(pos) {
            return Some(hit);
        }
    }
    for f in [Field::Prompt, Field::At, Field::Branch] {
        let r = field(screen, f);
        if r.contains(pos) {
            return Some(Hit::Field(f, usize::from(pos.y - r.y), usize::from(pos.x - r.x)));
        }
    }
    None
}

fn draw_editor(f: &mut Frame, screen: Rect, editor: &Editor, field: Field, focused: bool, style: Style) {
    let r = self::field(screen, field);
    let width = usize::from(r.width.max(1));
    let (line, col) = editor.position(width);
    let start = first_line(editor, r);
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
    f.render_widget(Paragraph::new(lines.join("\n")).style(style.add_modifier(Modifier::BOLD)), r);
    if focused && !r.is_empty() {
        f.set_cursor_position(Position::new(
            r.x + u16::try_from(col).unwrap_or_default().min(r.width.saturating_sub(1)),
            r.y + u16::try_from(line - start).unwrap_or_default().min(r.height.saturating_sub(1)),
        ));
    }
}

pub(in crate::ui) fn draw(f: &mut Frame, view: &View, form: &Form) {
    let screen = f.area();
    let area = area(screen);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, "Schedule an agent"), area);
    let dim = Style::default().fg(view.muted);
    for (r, label) in [
        (row(screen, 0, 1), "Prompt".to_string()),
        (row(screen, 5, 1), format!("When · {} · 14:30, tomorrow 9:00, in 2h", form.zone)),
        (row(screen, 15, 2), "Queued agents always accept their folder trust question.".to_string()),
    ] {
        f.render_widget(Paragraph::new(Span::styled(label, dim)).wrap(Wrap { trim: true }), r);
    }
    for (field, editor) in [(Field::Prompt, &form.prompt), (Field::At, &form.at), (Field::Branch, &form.branch)] {
        draw_editor(
            f,
            screen,
            editor,
            field,
            form.field == field && !form.busy,
            Style::default().bg(view.surface()).fg(if view.light { Color::Black } else { Color::White }),
        );
    }
    for (r, label) in [
        (row(screen, 9, 1), format!("Place: {} ›", form.place)),
        (row(screen, 10, 1), format!("Agent: {} ›", form.agent)),
        (row(screen, 11, 1), format!("Mode: {} ›", form.mode)),
        (row(screen, 12, 1), format!("{} New worktree · branch below", if form.worktree { "✓" } else { "○" })),
    ] {
        let style = if hovered(view, r) { Style::default().fg(Color::Cyan) } else { dim };
        f.render_widget(Paragraph::new(Span::styled(label, style)), r);
    }
    let [claude, codex] = reset(screen);
    draw_secondary(f, view, claude, "claude reset");
    draw_secondary(f, view, codex, "codex reset");
    if let Some(note) = &form.note {
        let r = self::note(screen);
        f.render_widget(Clear, r);
        f.render_widget(
            Paragraph::new(note.as_str())
                .style(Style::default().fg(if form.busy { view.muted } else { Color::Red }))
                .wrap(Wrap { trim: true }),
            r,
        );
    }
    draw_dialog_buttons(f, view, buttons(screen), "schedule", "cancel");
}
