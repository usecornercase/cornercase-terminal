use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::{
    CANCEL_LABEL, INPUT_PROMPT, buttons_in, draw_dialog_buttons, first_visible, form_inner, list_item, pick_background,
};
use crate::text::truncate_left;
use crate::ui::View;
use crate::ui::widgets::{overlay_block, rail};

const PICKER_WIDTH: u16 = 72;

const PICKER_HEIGHT: u16 = 24;

pub fn picker_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(PICKER_WIDTH);
    let height = area.height.saturating_sub(2).min(PICKER_HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub(in crate::ui) fn picker_rows(picker: Rect) -> [Rect; 4] {
    let [input, _, list, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(picker));
    [input, list, note, buttons]
}

pub fn picker_list(picker: Rect) -> Rect {
    picker_rows(picker)[1]
}

pub fn picker_buttons(picker: Rect, submit: &str) -> [Rect; 2] {
    buttons_in(picker_rows(picker)[3], submit, CANCEL_LABEL)
}

pub fn picker_item(picker: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    list_item(picker_list(picker), items, scroll, i)
}

pub fn list_item_at(list_box: Rect, items: usize, scroll: usize, i: usize) -> Position {
    list_item(picker_list(list_box), items, scroll, i).as_position()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerHit {
    Item(usize),
    Submit,
    Cancel,
}

pub fn picker_hit(area: Rect, submit: &str, items: usize, scroll: usize, pos: Position) -> Option<PickerHit> {
    list_box_hit(picker_area(area), submit, items, scroll, pos)
}

fn list_box_hit(picker: Rect, submit: &str, items: usize, scroll: usize, pos: Position) -> Option<PickerHit> {
    let [s, c] = picker_buttons(picker, submit);
    if s.contains(pos) {
        return Some(PickerHit::Submit);
    }
    if c.contains(pos) {
        return Some(PickerHit::Cancel);
    }
    let list = picker_list(picker);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(PickerHit::Item(i))
}

pub struct Picker {
    pub title: &'static str,
    pub path: String,
    pub filter: String,
    pub items: Vec<Entry>,
    pub selected: Option<usize>,
    pub scroll: usize,
    pub hint: String,
    pub error: Option<String>,
    pub submit: &'static str,
    pub empty: &'static str,
}

pub struct Entry {
    pub name: String,
    pub branch: Option<String>,
}

pub(in crate::ui) fn draw_picker(f: &mut Frame, view: &View, picker: &Picker) {
    let r = picker_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, picker.title), r);
    let [input, list, note, _] = picker_rows(r);
    let dim = Style::default().fg(view.muted);

    let max = usize::from(input.width).saturating_sub(INPUT_PROMPT.chars().count() + 1);
    let filter = truncate_left(&picker.filter, max);
    let path = truncate_left(&picker.path, max.saturating_sub(filter.chars().count()));
    let typed = path.chars().count() + filter.chars().count();
    let cursor_x = input.x + u16::try_from(INPUT_PROMPT.chars().count() + typed).unwrap_or(0);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(INPUT_PROMPT, Style::default().fg(Color::Cyan)),
            Span::raw(path),
            Span::styled(filter, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        input,
    );

    if picker.items.is_empty() {
        let text = if picker.filter.is_empty() { picker.empty } else { "no matches" };
        f.render_widget(Paragraph::new(Span::styled(format!(" {text}"), dim)), list);
    }
    for (i, item) in picker.items.iter().enumerate() {
        let row = picker_item(r, picker.items.len(), picker.scroll, i);
        if row.is_empty() {
            continue;
        }
        let selected = picker.selected == Some(i);
        let bg = pick_background(view, selected, row);
        let style = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
        let branch = item.branch.as_deref().unwrap_or_default();
        let branch_width = branch.chars().count();
        let max = usize::from(row.width).saturating_sub(branch_width + 3);
        let name = truncate_left(&item.name, max);
        let gap = usize::from(row.width).saturating_sub(name.chars().count() + branch_width + 2);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                rail(selected, bg),
                Span::styled(format!("{name}{}", " ".repeat(gap)), style),
                Span::styled(format!("{branch} "), bg.fg(view.muted)),
            ]))
            .style(bg),
            row,
        );
    }

    match &picker.error {
        Some(error) => f.render_widget(
            Paragraph::new(Span::styled(
                truncate_left(error, usize::from(note.width)),
                Style::default().fg(Color::Red),
            )),
            note,
        ),
        None => f.render_widget(
            Paragraph::new(Span::styled(truncate_left(&picker.hint, usize::from(note.width)), dim)),
            note,
        ),
    }

    draw_dialog_buttons(f, view, picker_buttons(r, picker.submit), picker.submit, CANCEL_LABEL);

    if cursor_x < input.right() {
        f.set_cursor_position(Position::new(cursor_x, input.y));
    }
}
