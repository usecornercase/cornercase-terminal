use std::cmp::Ordering;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::View;
use super::chrome::draw_back;
use super::drop::landed;
use super::layout::Areas;
use super::rows::{
    Band, GAP, RowButton, Rows, Tags, draw_hidden, draw_landing, draw_rail, draw_row_buttons, marker, row_button,
    row_close_button, row_menu_button, row_rect,
};
use super::theme::{severity_color, status_icon};
use super::widgets::{button_style, dim, draw_band, draw_button, draw_title, middle};
use crate::activity::{self, Status};
use crate::text::truncate_right;
use crate::usage::Severity;

const SHELL_ICON: &str = "›";

const PROGRAM_ICON: &str = "▸";

pub const ICON_WIDTH: usize = 2;

const BEHIND_ICON: &str = "↓";

const REMOVING_LABEL: &str = "removing…";

const CONTEXT_SEPARATOR: &str = " · ";

const MIN_MODEL_WIDTH: usize = 4;

pub(super) const MB: u64 = 1 << 20;

pub(super) const GB: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceRow {
    Gap,
    Workspace(usize),
    Tab(usize, usize),
    NewTab(usize),
    Landing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabLines {
    lines: Vec<Vec<u16>>,
    listed: bool,
}

impl TabLines {
    pub fn new(lines: Vec<Vec<u16>>, listed: bool) -> Self {
        let lines = if listed { lines } else { vec![Vec::new(); lines.len()] };
        Self { lines, listed }
    }
}

impl From<Vec<Vec<u16>>> for TabLines {
    fn from(lines: Vec<Vec<u16>>) -> Self {
        Self::new(lines, true)
    }
}

pub fn workspace_rows(tabs: &TabLines) -> Vec<WorkspaceRow> {
    tabs.lines
        .iter()
        .enumerate()
        .flat_map(|(w, lines)| {
            (w > 0)
                .then_some(WorkspaceRow::Gap)
                .into_iter()
                .chain(std::iter::once(WorkspaceRow::Workspace(w)))
                .chain((0..lines.len()).map(move |t| WorkspaceRow::Tab(w, t)))
                .chain(tabs.listed.then_some(WorkspaceRow::NewTab(w)))
        })
        .collect()
}

fn workspace_rows_layout(list: Rect, pitch: u16, rows: &[WorkspaceRow], tabs: &TabLines, scroll: usize) -> Rows {
    let heights = rows
        .iter()
        .map(|row| match *row {
            WorkspaceRow::Gap | WorkspaceRow::Landing => GAP,
            WorkspaceRow::Tab(w, t) => pitch.max(tabs.lines[w][t]),
            WorkspaceRow::Workspace(_) | WorkspaceRow::NewTab(_) => pitch,
        })
        .collect();
    Rows { list, heights, button: pitch, scroll }
}

pub fn workspace_layout(list: Rect, pitch: u16, tabs: &TabLines, scroll: usize) -> Rows {
    workspace_rows_layout(list, pitch, &workspace_rows(tabs), tabs, scroll)
}

pub fn new_workspace_button(list: Rect, pitch: u16, tabs: &TabLines) -> Rect {
    workspace_layout(list, pitch, tabs, 0).button()
}

pub fn workspace_row(list: Rect, pitch: u16, tabs: &TabLines, scroll: usize, row: WorkspaceRow) -> Rect {
    row_rect(&workspace_layout(list, pitch, tabs, scroll), &workspace_rows(tabs), &row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceHit {
    Workspace(usize),
    CloseWorkspace(usize),
    WorkspaceMenu(usize),
    Tab(usize, usize),
    CloseTab(usize, usize),
    TabMenu(usize, usize),
    NewTab(usize),
    NewWorkspace,
}

impl WorkspaceHit {
    pub fn workspace(self) -> Option<usize> {
        match self {
            Self::Workspace(w)
            | Self::CloseWorkspace(w)
            | Self::WorkspaceMenu(w)
            | Self::Tab(w, _)
            | Self::CloseTab(w, _)
            | Self::TabMenu(w, _)
            | Self::NewTab(w) => Some(w),
            Self::NewWorkspace => None,
        }
    }
}

pub fn workspace_hit(list: Rect, pitch: u16, tabs: &TabLines, scroll: usize, pos: Position) -> Option<WorkspaceHit> {
    if !list.contains(pos) {
        return None;
    }
    let layout = workspace_layout(list, pitch, tabs, scroll);
    if layout.button().contains(pos) {
        return Some(WorkspaceHit::NewWorkspace);
    }
    let i = layout.at(pos)?;
    let button = row_button(layout.item(i), pitch, pos);
    Some(match (workspace_rows(tabs)[i], button) {
        (WorkspaceRow::Gap | WorkspaceRow::Landing, _) => return None,
        (WorkspaceRow::Workspace(w), Some(RowButton::Close)) => WorkspaceHit::CloseWorkspace(w),
        (WorkspaceRow::Workspace(w), Some(RowButton::Menu)) => WorkspaceHit::WorkspaceMenu(w),
        (WorkspaceRow::Workspace(w), None) => WorkspaceHit::Workspace(w),
        (WorkspaceRow::Tab(w, t), Some(RowButton::Close)) => WorkspaceHit::CloseTab(w, t),
        (WorkspaceRow::Tab(w, t), Some(RowButton::Menu)) => WorkspaceHit::TabMenu(w, t),
        (WorkspaceRow::Tab(w, t), None) => WorkspaceHit::Tab(w, t),
        (WorkspaceRow::NewTab(w), _) => WorkspaceHit::NewTab(w),
    })
}

pub struct WorkspaceEntry {
    pub name: String,
    pub tabs: Vec<TabEntry>,
    pub behind: u32,
    pub removing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabEntry {
    pub name: String,
    pub status: Option<Status>,
    pub running: bool,
    pub details: Details,
    pub others: usize,
}

impl TabEntry {
    pub fn icon(&self, muted: Color) -> Span<'static> {
        match self.status {
            Some(status) => status_icon(muted, status),
            None => Span::styled(if self.running { PROGRAM_ICON } else { SHELL_ICON }, dim(muted)),
        }
    }

    pub fn prefix(&self, muted: Color, bg: Style) -> [Span<'static>; 2] {
        let icon = self.icon(muted);
        [Span::styled(icon.content, bg.patch(icon.style)), Span::styled(" ", bg)]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub model: Option<String>,
    pub percent: Option<u16>,
    pub memory: Option<u64>,
}

impl Details {
    pub fn lines(&self) -> u16 {
        1 + u16::from(self.model.is_some() || self.percent.is_some() || self.memory.is_some())
    }
}

impl From<&str> for TabEntry {
    fn from(name: &str) -> Self {
        Self::from(name.to_string())
    }
}

impl From<String> for TabEntry {
    fn from(name: String) -> Self {
        Self { name, status: None, running: false, details: Details::default(), others: 0 }
    }
}

pub(super) fn draw_workspaces(f: &mut Frame, view: &View, areas: &Areas) {
    if areas.back.is_empty() {
        draw_title(f, view.muted, areas.workspaces_title, "Workspaces");
    } else {
        let name = view.projects.get(view.active).filter(|_| view.has_project).map(|p| p.name.as_str());
        draw_back(f, view, areas, name.unwrap_or_default());
    }
    if !view.has_project {
        return;
    }

    let list = areas.workspaces_list;
    let tabs = view.tab_lines();
    let dim = Style::default().fg(view.muted);
    let accent = Style::default().fg(Color::Cyan);
    let base = workspace_layout(list, areas.pitch, &tabs, view.workspaces_scroll);
    let landing = view.workspaces_landing();
    let (rows, landing_line) = landed(workspace_rows(&tabs), &base, landing, WorkspaceRow::Gap, WorkspaceRow::Landing);
    let layout = workspace_rows_layout(list, areas.pitch, &rows, &tabs, base.first());
    for (i, &row) in rows.iter().enumerate() {
        let r = layout.item(i);
        if r.is_empty() {
            continue;
        }
        let guide = r.x.saturating_add(2);
        match row {
            WorkspaceRow::Gap => {}
            WorkspaceRow::Landing => {
                draw_landing(f, r, landing.map(|l| l.spot.indent()));
                if among_tabs(&rows, i) {
                    draw_guide(f, view.line(), guide, r, Guide::Bar);
                }
            }
            WorkspaceRow::Workspace(w) => {
                let bg = view.row_background(r, view.dragging_workspace_row(row));
                let band = Band { r, pitch: areas.pitch, lead: vec![Span::raw("  ")], bg };
                draw_workspace_band(f, view, band, &view.workspaces[w], w == view.active_workspace, true);
                draw_guide(f, view.line(), guide, r, Guide::Top);
            }
            WorkspaceRow::Tab(w, t) => {
                let active = w == view.active_workspace && view.active_tab == Some(t);
                let bg = view.row_background(r, active || view.dragging_workspace_row(row));
                let band = Band { r, pitch: areas.pitch, lead: vec![marker(active), Span::raw("  ")], bg };
                draw_tab_band(f, view, band, &view.workspaces[w].tabs[t], active);
                draw_guide(f, view.line(), guide, r, Guide::Tee);
                if active {
                    draw_rail(f, r);
                }
            }
            WorkspaceRow::NewTab(w) => {
                if !view.workspaces[w].removing {
                    draw_button(f, r, "   ", "+ Tab", button_style(view, r, dim, Color::Cyan));
                }
                draw_guide(f, view.line(), guide, r, Guide::End);
            }
        }
    }

    draw_hidden(f, view.muted, &layout, &rows, |r| matches!(r, WorkspaceRow::Workspace(_) | WorkspaceRow::Tab(..)));

    let r = layout.button();
    draw_button(f, r, " ", "+ New workspace", button_style(view, r, accent, Color::Cyan));
    draw_landing(f, landing_line, landing.map(|l| l.spot.indent()));
}

#[derive(Clone, Copy)]
pub(super) enum Guide {
    Top,
    Tee,
    End,
    Bar,
}

pub(super) fn draw_guide(f: &mut Frame, colour: Color, x: u16, r: Rect, guide: Guide) {
    if x >= r.right() {
        return;
    }
    let mid = middle(r).y;
    let buf = f.buffer_mut();
    for y in r.top()..r.bottom() {
        let symbol = match (guide, y.cmp(&mid)) {
            (Guide::Tee, Ordering::Equal) => "├",
            (Guide::End, Ordering::Equal) => "└",
            (Guide::Bar, _)
            | (Guide::Top | Guide::Tee, Ordering::Greater)
            | (Guide::Tee | Guide::End, Ordering::Less) => "│",
            _ => continue,
        };
        let cell = &mut buf[(x, y)];
        if cell.symbol() == " " {
            cell.set_symbol(symbol).set_fg(colour);
        }
    }
}

fn among_tabs(rows: &[WorkspaceRow], i: usize) -> bool {
    let above = i.checked_sub(1).and_then(|j| rows.get(j)).and_then(|row| match row {
        WorkspaceRow::Workspace(w) | WorkspaceRow::Tab(w, _) => Some(*w),
        _ => None,
    });
    let below = rows.get(i + 1).and_then(|row| match row {
        WorkspaceRow::Tab(w, _) | WorkspaceRow::NewTab(w) => Some(*w),
        _ => None,
    });
    above.is_some() && above == below
}

pub(super) fn draw_workspace_band(
    f: &mut Frame,
    view: &View,
    band: Band,
    entry: &WorkspaceEntry,
    active: bool,
    badge: bool,
) {
    let colour = match (entry.removing, active) {
        (true, _) => view.muted,
        (false, true) => Color::White,
        (false, false) => Color::Gray,
    };
    let style = Style::default().fg(colour).add_modifier(Modifier::BOLD);
    let room = band.room();
    let marks = if entry.removing {
        Tags::fit(vec![Span::styled(REMOVING_LABEL, Style::default().fg(view.muted))], room)
    } else {
        let badge = badge
            .then(|| activity::attention(entry.tabs.iter().map(|t| t.status)))
            .flatten()
            .map(|status| status_icon(view.muted, status));
        let behind = Some(entry.behind)
            .filter(|n| *n > 0)
            .map(|n| Span::styled(format!("{BEHIND_ICON}{n}"), Style::default().fg(Color::Yellow)));
        Tags::fit(badge.into_iter().chain(behind).collect(), room)
    };
    let name = truncate_right(&entry.name, room.saturating_sub(marks.reserved()));
    let used = name.chars().count();
    let mut line = band.lead;
    line.push(Span::styled(name, style));
    marks.push_onto(&mut line, used, room);
    draw_band(f, band.r, Line::from(line), band.bg);
    if !entry.removing {
        draw_row_buttons(f, view, band.r, band.pitch, band.bg);
    }
}

pub(super) fn draw_tab_band(f: &mut Frame, view: &View, band: Band, tab: &TabEntry, active: bool) {
    let style = Style::default().fg(if active { Color::White } else { Color::Gray });
    let indent = band.lead_width() + ICON_WIDTH;
    let max = band.room().saturating_sub(ICON_WIDTH);
    let others = (tab.others > 0).then(|| Span::styled(format!("+{}", tab.others), Style::default().fg(view.muted)));
    let marks = Tags::fit(others.into_iter().collect(), max);
    let name = if marks.is_empty() {
        truncate_right(&tab.name, max)
    } else {
        let room = max.saturating_sub(marks.reserved());
        truncate_right(&tab.name, room).chars().take(room).collect()
    };
    let used = name.chars().count();
    let mut line = band.lead;
    line.extend(tab.prefix(view.muted, Style::default()));
    line.push(Span::styled(name, style));
    marks.push_onto(&mut line, used, max);
    draw_band(f, band.r, Line::from(line), band.bg);
    if tab.details.lines() > 1 {
        let buttons = row_menu_button(band.r, band.pitch).union(row_close_button(band.r, band.pitch));
        draw_details(f, view.muted, (band.r, buttons), &tab.details, None, indent);
    }
    draw_row_buttons(f, view, band.r, band.pitch, band.bg);
}

pub(super) fn draw_details(
    f: &mut Frame,
    muted: Color,
    (row, buttons): (Rect, Rect),
    details: &Details,
    agent: Option<&str>,
    indent: usize,
) {
    let r = Rect { y: middle(row).y + 1, height: 1, ..row }.intersection(row);
    let reserved = if buttons.bottom() > r.y { usize::from(buttons.width) } else { 0 };
    let dim = Style::default().fg(muted);
    let percent = details.percent.map(|used| {
        let level = match Severity::of(used) {
            Severity::Normal => dim,
            severity => Style::default().fg(severity_color(severity)),
        };
        Span::styled(format!("{used}%"), level)
    });
    let memory = details.memory.map(|bytes| Span::styled(memory_size(bytes), dim));
    let lead = agent.map(|agent| Span::styled(agent.to_string(), dim));
    let fixed: Vec<Span> = percent.into_iter().chain(memory).collect();
    let separator = CONTEXT_SEPARATOR.chars().count();
    let fixed_width = lead.iter().chain(&fixed).map(|s| s.width() + separator).sum::<usize>();
    let free = usize::from(r.width).saturating_sub(reserved + 1);
    let indent = indent.min(free.saturating_sub(fixed_width.saturating_sub(separator)));
    let model_room = free.saturating_sub(indent + fixed_width);
    let model = details
        .model
        .as_ref()
        .filter(|_| (fixed.is_empty() && lead.is_none()) || model_room >= MIN_MODEL_WIDTH)
        .map(|model| Span::styled(truncate_right(model, model_room), dim));
    let parts = lead.into_iter().chain(model).chain(fixed);
    let mut line = vec![Span::raw(" ".repeat(indent))];
    line.extend(parts.flat_map(|part| [Span::styled(CONTEXT_SEPARATOR, dim), part]).skip(1));
    f.render_widget(Paragraph::new(Line::from(line)), r);
}

pub(super) fn memory_size(bytes: u64) -> String {
    let megabytes = bytes.saturating_add(MB / 2) / MB;
    if megabytes < 1024 {
        return format!("{megabytes} MB");
    }
    let tenths = bytes.saturating_mul(10).saturating_add(GB / 2) / GB;
    format!("{}.{} GB", tenths / 10, tenths % 10)
}
