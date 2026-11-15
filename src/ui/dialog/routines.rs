use ratatui::Frame;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};

use super::{
    centred_in, draw_row_button, draw_secondary, draw_submit, item_at, list_item_by, list_note_buttons,
    pick_background, right_aligned, row_button_rects,
};
use crate::bots::routines::{Button, Hit};
use crate::text::truncate_right;
use crate::ui::COMPACT_PITCH;
use crate::ui::View;
use crate::ui::widgets::{button_width, hovered, overlay_block, rail};

const PITCH: u16 = 2;

fn pitch(compact: bool) -> u16 {
    if compact { COMPACT_PITCH } else { PITCH }
}

const WIDE: u16 = 100;

const HIGH: u16 = 24;

const NEW_LABEL: &str = "+ routine";

const DONE_LABEL: &str = "done";

const LEAD: usize = 4;

pub(crate) struct BotRoutines {
    pub(crate) title: String,
    pub(crate) rows: Vec<RoutineRow>,
    pub(crate) selected: Option<usize>,
    pub(crate) scroll: usize,
    pub(crate) hint: String,
    pub(crate) empty: String,
    pub(crate) compact: bool,
}

pub(crate) struct RoutineRow {
    pub(crate) name: String,
    pub(crate) when: String,
    pub(crate) state: Option<String>,
    pub(crate) colour: Option<Color>,
    pub(crate) at: String,
    pub(crate) paused: bool,
    pub(crate) buttons: Vec<Button>,
}

pub(crate) fn routines_area(screen: Rect, compact: bool) -> Rect {
    if compact { screen } else { centred_in(screen, WIDE, HIGH) }
}

pub(crate) fn routines_room(screen: Rect, compact: bool) -> (usize, Rect) {
    let [list, ..] = list_note_buttons(routines_area(screen, compact));
    (usize::from(list.height / pitch(compact)), list)
}

fn button_rects(row: &RoutineRow, rect: Rect) -> Vec<(Button, Rect)> {
    let labels: Vec<&str> = row.buttons.iter().map(|button| button.label()).collect();
    row.buttons.iter().copied().zip(row_button_rects(rect, &labels)).collect()
}

#[cfg(test)]
pub(crate) fn routine_button(screen: Rect, view: &BotRoutines, row: usize, button: Button) -> Rect {
    let (_, list) = routines_room(screen, view.compact);
    let Some(shown) = view.rows.get(row) else { return Rect::default() };
    let rect = list_item_by(list, view.rows.len(), view.scroll, row, pitch(view.compact));
    if rect.is_empty() {
        return Rect::default();
    }
    button_rects(shown, rect).into_iter().find(|(b, _)| *b == button).map_or_else(Rect::default, |(_, r)| r)
}

fn bottom(area: Rect) -> Vec<Rect> {
    let [.., row] = list_note_buttons(area);
    right_aligned(row, &[NEW_LABEL, DONE_LABEL], button_width, 1)
}

pub(crate) fn routines_hit(screen: Rect, view: &BotRoutines, pos: Position) -> Option<Hit> {
    let area = routines_area(screen, view.compact);
    let rects = bottom(area);
    if rects[0].contains(pos) {
        return Some(Hit::New);
    }
    if rects[1].contains(pos) {
        return Some(Hit::Done);
    }
    let [list, ..] = list_note_buttons(area);
    let (i, rect) = item_at(list, (view.rows.len(), view.scroll), pitch(view.compact), pos)?;
    let found = button_rects(view.rows.get(i)?, rect).into_iter().find(|(_, r)| r.contains(pos));
    Some(found.map_or(Hit::Row(i), |(button, _)| Hit::Button(i, button)))
}

pub(in crate::ui) fn draw_routines(f: &mut Frame, view: &View, routines: &BotRoutines) {
    let area = routines_area(f.area(), routines.compact);
    f.render_widget(Clear, area);
    f.render_widget(overlay_block(view.muted, &routines.title), area);
    let [list, note, _] = list_note_buttons(area);
    let dim = Style::default().fg(view.muted);
    if routines.rows.is_empty() {
        let text = Paragraph::new(Span::styled(routines.empty.as_str(), dim)).wrap(Wrap { trim: false });
        f.render_widget(text, list.inner(Margin::new(1, 0)));
    }
    for (i, row) in routines.rows.iter().enumerate() {
        let rect = list_item_by(list, routines.rows.len(), routines.scroll, i, pitch(routines.compact));
        if !rect.is_empty() {
            draw_row(f, view, routines, (row, routines.selected == Some(i)), rect);
        }
    }
    let hint = truncate_right(&routines.hint, usize::from(note.width));
    f.render_widget(Paragraph::new(Span::styled(hint, dim)), note);
    let rects = bottom(area);
    draw_submit(f, view, rects[0], NEW_LABEL);
    draw_secondary(f, view, rects[1], DONE_LABEL);
}

fn draw_row(f: &mut Frame, view: &View, routines: &BotRoutines, (row, selected): (&RoutineRow, bool), rect: Rect) {
    let bg = pick_background(view, selected, rect);
    let width = usize::from(rect.width);
    let when = truncate_right(&row.when, width / 2);
    let room = width.saturating_sub(LEAD + when.chars().count() + 1);
    let name = truncate_right(&row.name, room);
    let gap = width.saturating_sub(1 + name.chars().count() + when.chars().count() + 1);
    let weight = if selected { Modifier::BOLD } else { Modifier::empty() };
    let name_style = if row.paused { bg.fg(view.muted) } else { bg.add_modifier(weight) };
    let first = Line::from(vec![
        rail(selected, bg),
        Span::styled(name, name_style),
        Span::styled(" ".repeat(gap), bg),
        Span::styled(format!("{when} "), bg.fg(view.muted)),
    ]);
    let mut second = vec![rail(selected, bg), Span::styled("  ", bg)];
    let shown = routines.compact || selected || hovered(view, rect);
    let taken = if shown && !routines.compact {
        button_rects(row, rect).iter().map(|(_, r)| usize::from(r.width)).sum::<usize>() + 1
    } else {
        0
    };
    second.extend(last_run(row, view, bg, width.saturating_sub(LEAD + taken)));
    let lines = vec![first, Line::from(second)];
    f.render_widget(Paragraph::new(lines).style(bg), rect);
    if shown {
        for (button, r) in button_rects(row, rect) {
            draw_row_button(f, view, r, button.label(), bg);
        }
    }
}

fn last_run(row: &RoutineRow, view: &View, bg: Style, room: usize) -> Vec<Span<'static>> {
    let dim = bg.fg(view.muted);
    let Some(state) = &row.state else { return vec![Span::styled("not run yet", dim)] };
    if row.at.is_empty() {
        return vec![Span::styled(state.clone(), dim)];
    }
    let head = format!("last run {state} · ");
    let at = truncate_right(&row.at, room.saturating_sub(head.chars().count()));
    vec![
        Span::styled("last run ", dim),
        Span::styled(state.clone(), bg.fg(row.colour.unwrap_or(view.muted))),
        Span::styled(format!(" · {at}"), dim),
    ]
}
