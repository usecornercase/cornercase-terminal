use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::form::form_area;
use super::{draw_submit, form_inner};
use crate::project::group::{GROUP_COLOURS, GROUP_ICONS, GroupEntry};
use crate::ui::View;
use crate::ui::sidebar::group_header;
use crate::ui::widgets::{hovered, overlay_block, update_button};

const ICONS_PER_ROW: usize = 6;

const COLOURS_PER_ROW: usize = 8;

pub(in crate::ui) const ICON_CELL: u16 = 3;

const COLOUR_CELL: u16 = 4;

fn style_rows(area: Rect) -> [Rect; 5] {
    let [icon_label, icons, colour_label, colours, _, last] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(form_inner(form_area(area)));
    [icon_label, icons, colour_label, colours, last]
}

fn grid_cell(grid: Rect, per_row: usize, width: u16, i: usize) -> Rect {
    let (row, col) = (u16::try_from(i / per_row).unwrap_or(u16::MAX), u16::try_from(i % per_row).unwrap_or(u16::MAX));
    Rect::new(grid.x.saturating_add(col.saturating_mul(width)), grid.y.saturating_add(row), width, 1).intersection(grid)
}

#[cfg(test)]
pub(crate) fn style_icon(area: Rect, i: usize) -> Rect {
    grid_cell(style_rows(area)[1], ICONS_PER_ROW, ICON_CELL, i)
}

#[cfg(test)]
pub(crate) fn style_colour(area: Rect, i: usize) -> Rect {
    grid_cell(style_rows(area)[3], COLOURS_PER_ROW, COLOUR_CELL, i)
}

#[cfg(test)]
pub(crate) fn style_done(area: Rect) -> Rect {
    update_button(style_rows(area)[4], crate::settings::DONE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleHit {
    Icon(usize),
    Colour(usize),
    Done,
}

pub(crate) fn style_hit(area: Rect, pos: Position) -> Option<StyleHit> {
    let [_, icons, _, colours, last] = style_rows(area);
    if update_button(last, crate::settings::DONE).contains(pos) {
        return Some(StyleHit::Done);
    }
    let cell = |grid, per_row, width, count| (0..count).find(|&i| grid_cell(grid, per_row, width, i).contains(pos));
    cell(icons, ICONS_PER_ROW, ICON_CELL, GROUP_ICONS.len())
        .map(StyleHit::Icon)
        .or_else(|| cell(colours, COLOURS_PER_ROW, COLOUR_CELL, GROUP_COLOURS.len()).map(StyleHit::Colour))
}

fn draw_icons(f: &mut Frame, view: &View, group: &GroupEntry, icons: Rect) {
    let selected = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    for (i, &icon) in GROUP_ICONS.iter().enumerate() {
        let cell = grid_cell(icons, ICONS_PER_ROW, ICON_CELL, i);
        let style = if icon == group.icon {
            selected
        } else if hovered(view, cell) {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Indexed(group.colour))
        };
        f.render_widget(Paragraph::new(Span::styled(format!(" {icon} "), style)), cell);
    }
}

fn draw_colours(f: &mut Frame, view: &View, group: &GroupEntry, colours: Rect) {
    let dim = Style::default().fg(view.muted);
    for (i, &colour) in GROUP_COLOURS.iter().enumerate() {
        let cell = grid_cell(colours, COLOURS_PER_ROW, COLOUR_CELL, i);
        let (open, close, edge) = if colour == group.colour {
            ("[", "]", Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD))
        } else if hovered(view, cell) {
            ("[", "]", dim)
        } else {
            (" ", " ", dim)
        };
        let line = Line::from(vec![
            Span::styled(open, edge),
            Span::styled("██", Style::default().fg(Color::Indexed(colour))),
            Span::styled(close, edge),
        ]);
        f.render_widget(Paragraph::new(line), cell);
    }
}

pub(in crate::ui) fn draw_group_style(f: &mut Frame, view: &View, group: &GroupEntry) {
    let area = f.area();
    let r = form_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &group.name), r);
    let [icon_label, icons, colour_label, colours, last] = style_rows(area);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled("icon", dim)), icon_label);
    f.render_widget(Paragraph::new(Span::styled("colour", dim)), colour_label);
    draw_icons(f, view, group, icons);
    draw_colours(f, view, group, colours);
    let done = update_button(last, crate::settings::DONE);
    let max = usize::from(last.width.saturating_sub(done.width)).saturating_sub(5);
    let preview = Line::from(vec![Span::styled("▾ ", dim), group_header(group, max)]);
    f.render_widget(Paragraph::new(preview), last);
    draw_submit(f, view, done, crate::settings::DONE);
}
