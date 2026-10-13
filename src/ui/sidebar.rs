use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Clear;

use super::View;
use super::drop::landed;
use super::layout::Areas;
use super::rows::{
    Band, GAP, RowButton, Rows, Tags, arrow, draw_hidden, draw_landing, draw_rail, draw_row_buttons, marker,
    row_button, row_close_button, row_rect,
};
use super::theme::status_icon;
use super::widgets::{button_style, draw_band, draw_button, draw_title};
use crate::agents::activity::{self, Status};
use crate::project::group::GroupEntry;
use crate::text::truncate_right;

pub const NEW_BUTTON: &str = "+ New";

const GROUP_INDENT: &str = "  ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarRow {
    Gap,
    Group(usize),
    Project(usize),
    Landing,
}

pub fn sidebar_rows(groups: &[Option<usize>], collapsed: &[bool]) -> Vec<SidebarRow> {
    let in_group = |g: Option<usize>| {
        groups.iter().enumerate().filter(move |(_, group)| **group == g).map(|(p, _)| SidebarRow::Project(p))
    };
    let mut rows: Vec<SidebarRow> = in_group(None).collect();
    for (g, &folded) in collapsed.iter().enumerate() {
        if !rows.is_empty() {
            rows.push(SidebarRow::Gap);
        }
        rows.push(SidebarRow::Group(g));
        if !folded {
            rows.extend(in_group(Some(g)));
        }
    }
    rows
}

pub(crate) fn active_row(rows: &[SidebarRow], project: usize, group: Option<usize>) -> Option<usize> {
    let find = |row: SidebarRow| rows.iter().position(|r| *r == row);
    find(SidebarRow::Project(project)).or_else(|| find(SidebarRow::Group(group?)))
}

pub(crate) fn project_rows(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize) -> Rows {
    let heights = rows.iter().map(|r| if matches!(r, SidebarRow::Gap | SidebarRow::Landing) { GAP } else { pitch });
    Rows { list, heights: heights.collect(), button: pitch, scroll }
}

pub(crate) fn entry_row(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize, row: SidebarRow) -> Rect {
    row_rect(&project_rows(list, pitch, rows, scroll), rows, &row)
}

pub fn close_button(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize, row: SidebarRow) -> Rect {
    row_close_button(entry_row(list, pitch, rows, scroll, row), pitch)
}

pub fn new_project_button(list: Rect, pitch: u16, rows: &[SidebarRow]) -> Rect {
    project_rows(list, pitch, rows, 0).button()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SidebarHit {
    Select(usize),
    Close(usize),
    Menu(usize),
    Group(usize),
    CloseGroup(usize),
    GroupMenu(usize),
    New,
}

pub(crate) fn sidebar_hit(
    list: Rect,
    pitch: u16,
    rows: &[SidebarRow],
    scroll: usize,
    pos: Position,
) -> Option<SidebarHit> {
    if !list.contains(pos) {
        return None;
    }
    let layout = project_rows(list, pitch, rows, scroll);
    if layout.button().contains(pos) {
        return Some(SidebarHit::New);
    }
    let i = layout.at(pos)?;
    let button = row_button(layout.item(i), pitch, pos);
    match (rows[i], button) {
        (SidebarRow::Gap | SidebarRow::Landing, _) => None,
        (SidebarRow::Group(g), Some(RowButton::Close)) => Some(SidebarHit::CloseGroup(g)),
        (SidebarRow::Group(g), Some(RowButton::Menu)) => Some(SidebarHit::GroupMenu(g)),
        (SidebarRow::Group(g), None) => Some(SidebarHit::Group(g)),
        (SidebarRow::Project(p), Some(RowButton::Close)) => Some(SidebarHit::Close(p)),
        (SidebarRow::Project(p), Some(RowButton::Menu)) => Some(SidebarHit::Menu(p)),
        (SidebarRow::Project(p), None) => Some(SidebarHit::Select(p)),
    }
}

pub(crate) struct ProjectEntry {
    pub(crate) name: String,
    pub(crate) workspaces: usize,
    pub(crate) group: Option<usize>,
    pub(crate) status: Option<Status>,
}

pub(super) fn group_header(group: &GroupEntry, max: usize) -> Span<'static> {
    let style = Style::default().fg(Color::Indexed(group.colour)).add_modifier(Modifier::BOLD);
    Span::styled(format!("{} {}", group.icon, truncate_right(&group.name, max)), style)
}

pub(super) fn draw_sidebar(f: &mut Frame, view: &View, areas: &Areas) {
    draw_title(f, view.muted, areas.title, "Projects");
    let sidebar = view.sidebar_rows();
    let base = project_rows(areas.list, areas.pitch, &sidebar, view.projects_scroll);
    let landing = view.sidebar_landing();
    let (sidebar, line) = landed(sidebar, &base, landing, SidebarRow::Gap, SidebarRow::Landing);
    let rows = project_rows(areas.list, areas.pitch, &sidebar, base.first());
    draw_entries(f, view, &rows, &sidebar, areas.pitch);
    draw_hidden(f, view.muted, &rows, &sidebar, |r| matches!(r, SidebarRow::Group(_) | SidebarRow::Project(_)));
    draw_new_project(f, view, rows.button());
    draw_landing(f, line, landing.map(|l| l.spot.indent()));
}

pub(super) fn draw_new_project(f: &mut Frame, view: &View, r: Rect) {
    f.render_widget(Clear, r);
    draw_button(f, r, " ", NEW_BUTTON, button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan));
}

fn group_attention(view: &View, g: usize) -> Option<Status> {
    activity::attention(view.projects.iter().filter(|p| p.group == Some(g)).map(|p| p.status))
}

fn draw_entries(f: &mut Frame, view: &View, rows: &Rows, sidebar: &[SidebarRow], pitch: u16) {
    let group = view.projects.get(view.active).and_then(|p| p.group);
    let active = active_row(sidebar, view.active, group).filter(|_| view.has_project);
    for (i, &row) in sidebar.iter().enumerate() {
        let r = rows.item(i);
        if r.is_empty() {
            continue;
        }
        match row {
            SidebarRow::Gap => {}
            SidebarRow::Landing => draw_landing(f, r, view.sidebar_landing().map(|l| l.spot.indent())),
            SidebarRow::Group(g) => {
                let bg = view.row_background(r, view.dragging_entry(row));
                draw_group(f, view, g, Band { r, pitch, lead: vec![marker(active == Some(i))], bg });
                if active == Some(i) {
                    draw_rail(f, r);
                }
            }
            SidebarRow::Project(p) => {
                let bg = view.row_background(r, p == view.active || view.dragging_entry(row));
                let indent = if view.projects[p].group.is_some() { GROUP_INDENT } else { "" };
                let lead = vec![marker(p == view.active), Span::raw(indent)];
                draw_project_band(f, view, Band { r, pitch, lead, bg }, p, true);
                if p == view.active {
                    draw_rail(f, r);
                }
            }
        }
    }
}

pub(super) fn draw_group(f: &mut Frame, view: &View, g: usize, band: Band) {
    let group = &view.groups[g];
    let count = if group.collapsed && view.counts {
        format!(" ({})", view.projects.iter().filter(|p| p.group == Some(g)).count())
    } else {
        String::new()
    };
    let badge = group
        .collapsed
        .then(|| group_attention(view, g))
        .flatten()
        .map(|status| status_icon(view.muted, status, view.spin));
    let room = band.room().saturating_sub(2);
    let used = 2 + count.chars().count();
    let marks = Tags::fit(badge.into_iter().collect(), room.saturating_sub(used));
    let max = room.saturating_sub(used + marks.reserved());
    let shown = used + truncate_right(&group.name, max).chars().count();
    let mut line = band.lead;
    line.extend([
        arrow(view.muted, group.collapsed),
        group_header(group, max),
        Span::styled(count, Style::default().fg(view.muted)),
    ]);
    marks.push_onto(&mut line, shown, room);
    draw_band(f, band.r, Line::from(line), band.bg);
    draw_row_buttons(f, view, band.r, band.pitch, band.bg);
}

pub(super) fn draw_project_band(f: &mut Frame, view: &View, band: Band, p: usize, summary: bool) {
    let entry = &view.projects[p];
    let title_style = if p == view.active {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let count = if summary && view.counts { format!(" ({})", entry.workspaces) } else { String::new() };
    let room = band.room();
    let badge = entry.status.filter(|_| summary).map(|status| status_icon(view.muted, status, view.spin));
    let marks = Tags::fit(badge.into_iter().collect(), room.saturating_sub(count.chars().count()));
    let max = room.saturating_sub(count.chars().count() + marks.reserved());
    let name = truncate_right(&entry.name, max);
    let shown = name.chars().count() + count.chars().count();
    let mut line = band.lead;
    line.extend([Span::styled(name, title_style), Span::styled(count, Style::default().fg(view.muted))]);
    marks.push_onto(&mut line, shown, room);
    draw_band(f, band.r, Line::from(line), band.bg);
    draw_row_buttons(f, view, band.r, band.pitch, band.bg);
}
