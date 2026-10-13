pub(super) mod form;
pub(super) mod group_style;
pub(super) mod issues;
pub(super) mod menu;
pub(super) mod picker;
pub(super) mod search;
pub(super) mod settings;
pub(super) mod update;
pub(super) mod usage;

use ratatui::Frame;
use ratatui::layout::{Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use super::RemoteAccess;
use super::View;
use super::dialog::form::{Confirm, Form, form_area};
use super::dialog::issues::{Issues, issues_area};
use super::dialog::picker::{Picker, picker_area};
use super::dialog::search::Search;
use super::dialog::settings::{Settings, settings_area};
use super::dialog::update::Update;
use super::dialog::usage::{Usage, usage_area};
use super::keys;
use super::remote_access;
use super::widgets::{button_width, hovered, tab_style};
use crate::project::group::GroupEntry;
use crate::text::truncate_left;

pub(super) const FORM_WIDTH: u16 = 64;

const FORM_PADDING: u16 = 1;

pub(super) const INPUT_PROMPT: &str = "› ";

pub(crate) const CANCEL_LABEL: &str = "cancel";

pub(super) fn form_inner(form: Rect) -> Rect {
    Block::bordered().inner(form).inner(Margin::new(FORM_PADDING, 0))
}

fn buttons_in(row: Rect, submit: &str, cancel: &str) -> [Rect; 2] {
    let cancel_width = button_width(cancel);
    let submit_width = button_width(submit);
    let cancel = Rect::new(row.right().saturating_sub(cancel_width), row.y, cancel_width, 1).intersection(row);
    let submit = Rect::new(cancel.x.saturating_sub(submit_width + 1), row.y, submit_width, 1).intersection(row);
    [submit, cancel]
}

fn first_visible(list: Rect, items: usize, scroll: usize) -> usize {
    scroll.min(items.saturating_sub(usize::from(list.height)))
}

pub(super) fn list_item(list: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    let Some(row) = i.checked_sub(first_visible(list, items, scroll)).filter(|_| i < items) else {
        return Rect::default();
    };
    let y = list.y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    Rect::new(list.x, y, list.width, 1).intersection(list)
}

fn row_of(row: Rect, x: u16, width: u16) -> Rect {
    Rect::new(x, row.y, width, 1).intersection(row)
}

pub(super) fn tabs_in(row: Rect, names: &[&str]) -> Vec<Rect> {
    let mut x = row.x;
    names
        .iter()
        .map(|name| {
            let r = row_of(row, x, button_width(name));
            x = x.saturating_add(button_width(name) + 1);
            r
        })
        .collect()
}

fn right_aligned(row: Rect, labels: &[&str], width: fn(&str) -> u16, gap: u16) -> Vec<Rect> {
    let mut x = row.right();
    let mut rects: Vec<Rect> = labels
        .iter()
        .rev()
        .map(|label| {
            x = x.saturating_sub(width(label));
            let r = row_of(row, x, width(label));
            x = x.saturating_sub(gap);
            r
        })
        .collect();
    rects.reverse();
    rects
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Note {
    Error(String),
    Busy(&'static str),
}

pub(crate) enum Overlay {
    Menu { at: Position, items: Vec<String> },
    Form(Form),
    GroupStyle(GroupEntry),
    Confirm(Confirm),
    Update(Update),
    Usage(Usage),
    Remote(RemoteAccess),
    Picker(Picker),
    Issues(Issues),
    Settings(Settings),
    Search(Search),
    Keys(keys::Keys),
}

impl Overlay {
    pub(crate) fn is_modal(&self) -> bool {
        !matches!(self, Self::Menu { .. } | Self::Search(_) | Self::Keys(_))
    }

    pub(crate) fn area(&self, area: Rect) -> Option<Rect> {
        match self {
            Self::Form(_) | Self::GroupStyle(_) | Self::Confirm(_) => Some(form_area(area)),
            Self::Update(_) | Self::Picker(_) => Some(picker_area(area)),
            Self::Usage(usage) => Some(usage_area(area, usage)),
            Self::Remote(remote) => Some(remote_access::area(area, remote)),
            Self::Issues(_) => Some(issues_area(area)),
            Self::Settings(_) => Some(settings_area(area)),
            Self::Menu { .. } | Self::Search(_) | Self::Keys(_) => None,
        }
    }
}

fn pick_background(view: &View, selected: bool, r: Rect) -> Style {
    if selected {
        Style::default().bg(view.surface())
    } else if hovered(view, r) {
        Style::default().bg(view.hover_fill())
    } else {
        Style::default()
    }
}

fn draw_note(f: &mut Frame, muted: Color, note: Option<&Note>, r: Rect) {
    match note {
        Some(Note::Error(text)) => f.render_widget(
            Paragraph::new(text.as_str()).style(Style::default().fg(Color::Red)).wrap(Wrap { trim: true }),
            r,
        ),
        Some(Note::Busy(text)) => {
            f.render_widget(Paragraph::new(Span::styled(*text, Style::default().fg(muted))), r);
        }
        None => {}
    }
}

fn dialog_button_style(view: &View, r: Rect, primary: bool) -> Style {
    match (primary, hovered(view, r)) {
        (true, true) => Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(Color::Cyan).bg(view.surface()).add_modifier(Modifier::BOLD),
        (false, true) => Style::default().fg(Color::Black).bg(Color::Gray),
        (false, false) => Style::default().fg(Color::Gray).bg(view.surface()),
    }
}

pub(super) fn draw_submit(f: &mut Frame, view: &View, r: Rect, label: &str) {
    let style = dialog_button_style(view, r, true);
    f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), r);
}

fn draw_secondary(f: &mut Frame, view: &View, r: Rect, label: &str) {
    let style = dialog_button_style(view, r, false);
    f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), r);
}

fn draw_dialog_buttons(f: &mut Frame, view: &View, [submit, cancel]: [Rect; 2], label: &str, cancel_label: &str) {
    draw_submit(f, view, submit, label);
    draw_secondary(f, view, cancel, cancel_label);
}

fn draw_input(f: &mut Frame, muted: Color, row: Rect, label: &str, value: &str) -> Option<Position> {
    let label = if label.is_empty() { String::new() } else { format!("{label} ") };
    let max = usize::from(row.width).saturating_sub(INPUT_PROMPT.chars().count() + label.chars().count() + 1);
    let value = truncate_left(value, max);
    let used = INPUT_PROMPT.chars().count() + label.chars().count() + value.chars().count();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(INPUT_PROMPT, Style::default().fg(Color::Cyan)),
            Span::styled(label, Style::default().fg(muted)),
            Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        row,
    );
    let x = row.x.saturating_add(u16::try_from(used).unwrap_or(u16::MAX));
    (x < row.right()).then(|| Position::new(x, row.y))
}

fn draw_tabs(f: &mut Frame, view: &View, rects: &[Rect], names: &[&str], active: usize) {
    for (i, (rect, name)) in rects.iter().zip(names).enumerate() {
        let style = tab_style(i == active, hovered(view, *rect), view.surface());
        f.render_widget(Paragraph::new(Span::styled(format!(" {name} "), style)), *rect);
    }
}
