use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use serde::{Deserialize, Serialize};

pub mod changes;
pub mod files;
pub mod keys;
pub mod remote;
pub mod remote_access;
mod stats;
pub mod tab_bar;
pub mod todo;

pub use remote_access::{RemoteAccess, button as remote_button, done as remote_done};

use crate::activity::{self, Status};
use crate::emulator::Snapshot;
use crate::host_theme::HostTheme;
use crate::markdown;
use crate::split::{Dir, Node, Place};
use crate::usage::Severity;

pub const SIDEBAR_WIDTH: u16 = 32;
pub const WORKSPACES_WIDTH: u16 = 26;
pub const PANE_PADDING: u16 = 1;
pub const MIN_COLUMN_WIDTH: u16 = 16;
pub const MIN_PANE_WIDTH: u16 = 20;
pub const COMPACT_WIDTH: u16 = 90;
pub const COMPACT_PITCH: u16 = 3;
pub const MIN_STACK_SECTION: u16 = 5;
const STACK_FOOTER: u16 = 6;
const COMPACT_BUTTON_WIDTH: u16 = 7;
const HEADER_HEIGHT: u16 = 2;
const GAP: u16 = 1;
const FORM_WIDTH: u16 = 64;
const FORM_HEIGHT: u16 = 10;
const FORM_PADDING: u16 = 1;
const PICKER_WIDTH: u16 = 72;
const PICKER_HEIGHT: u16 = 24;
const UPDATE_MESSAGE_HEIGHT: u16 = 3;
const ISSUES_WIDTH: u16 = 110;
const ISSUES_HEIGHT: u16 = 30;
const INPUT_PROMPT: &str = "› ";
const SEARCH_ICON: &str = " ⌕ ";
const SEARCH_PLACEHOLDER: &str = "search projects, workspaces, tabs";
const MENU_ICON: &str = "≡";
const SHELL_ICON: &str = "›";
const PROGRAM_ICON: &str = "▸";
pub const ICON_WIDTH: usize = 2;
const BACK_LABEL: &str = "‹ Projects";
pub const AGENTS_LABEL: &str = "Agents ›";
const AGENTS_TITLE: &str = "Agents";
const NO_AGENTS: &str = "no agents running";
const AGENT_LINES: u16 = 2;
const MIN_WORKSPACE_WIDTH: usize = 4;
pub const CRUMB_SEPARATOR: &str = " › ";
pub const CANCEL_LABEL: &str = "cancel";
const ISSUES_LABEL: &str = "Issues";
const USAGE_LABEL: &str = "Usage";
const USAGE_BAR: &str = "━";
const CHANGES_ICON: &str = "±";
const TODO_ICON: &str = "☐";
const FILES_ICON: &str = "▤";
pub const TODO_LABEL: &str = "TODO";
const TODO_DOT: &str = "●";
const UNDO_LABEL: &str = "undo";
const MOVE_LABEL: &str = "move here";
const SWAP_LABEL: &str = "swap";
const NO_TAB: &str = "no tab open";
const NO_TAB_HINT: &str = " opens a shell here";
const TAGLINE: &str = "every agent in its own corner";
const WELCOME_HINT: &str = " opens a folder";
pub const NEW_BUTTON: &str = "+ New";
const LOGO: [(&str, &str); 6] =
    [("   █████████", ""), ("▄▄▄▀▀▀▀▀▀▀▀▀", ""), ("███", ""), ("███   ", "██"), ("███   ", "██"), ("███   ", "▀▀")];
const BRAND_COLOR: Color = Color::Indexed(99);
const DARK_SURFACE: Color = Color::Indexed(236);
const LIGHT_SURFACE: Color = Color::Indexed(254);
const DARK_HOVER: Color = Color::Indexed(235);
const LIGHT_HOVER: Color = Color::Indexed(255);
const DARK_MUTED: Color = Color::Indexed(243);
const LIGHT_MUTED: Color = Color::Indexed(245);
const DARK_LINE: Color = Color::Indexed(239);
const LIGHT_LINE: Color = Color::Indexed(250);
const CLOSE_BUTTON_WIDTH: u16 = 3;
const COMPACT_CLOSE_WIDTH: u16 = 5;
const MENU_BUTTON_WIDTH: u16 = 2;
const COMPACT_MENU_WIDTH: u16 = 4;
const MIN_MENU_ROW_WIDTH: u16 = 20;
pub const ROW_MENU_ICON: &str = "⋯";
const TOAST_ICON: &str = " ✓ ";
const BUG_ICON: &str = " ✗ ";
const RESTART_ICON: &str = " ↻ ";
const TOAST_MARGIN: u16 = 1;
const BEHIND_ICON: &str = "↓";
const REMOVING_LABEL: &str = "removing…";
const CONTEXT_SEPARATOR: &str = " · ";
const MIN_MODEL_WIDTH: usize = 4;
const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;
const WAITING_COLOR: Color = Color::Indexed(208);
const GROUP_INDENT: &str = "  ";
pub const GROUP_ICONS: [char; 12] = ['●', '◉', '◐', '◆', '■', '▲', '▼', '★', '✦', '♥', '♣', '♠'];
pub const GROUP_COLOURS: [u8; 16] = [1, 9, 208, 214, 3, 11, 2, 10, 6, 14, 4, 12, 99, 5, 13, 205];
pub const GROUP_STYLES: [(char, u8); 8] =
    [('●', 12), ('◆', 2), ('★', 99), ('♥', 9), ('▲', 208), ('■', 214), ('✦', 205), ('◉', 6)];
const ICONS_PER_ROW: usize = 6;
const COLOURS_PER_ROW: usize = 8;
const ICON_CELL: u16 = 3;
const COLOUR_CELL: u16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Projects,
    Workspaces,
    Stack,
    Agents,
    Changes,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sidebar {
    SideBySide,
    #[default]
    ProjectsOnTop,
    WorkspacesOnTop,
    Tree,
}

impl Sidebar {
    pub const ALL: [Self; 4] = [Self::SideBySide, Self::ProjectsOnTop, Self::WorkspacesOnTop, Self::Tree];

    pub fn id(self) -> &'static str {
        match self {
            Self::SideBySide => "side_by_side",
            Self::ProjectsOnTop => "projects_on_top",
            Self::WorkspacesOnTop => "workspaces_on_top",
            Self::Tree => "tree",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::SideBySide => "projects and workspaces in two columns",
            Self::ProjectsOnTop => "one column, workspaces below projects",
            Self::WorkspacesOnTop => "one column, projects below workspaces",
            Self::Tree => "one list: projects, workspaces and tabs",
        }
    }

    pub fn choices() -> Vec<(&'static str, &'static str)> {
        Self::ALL.into_iter().map(|s| (s.id(), s.note())).collect()
    }

    pub fn from_setting(setting: &str) -> Self {
        from_setting(&Self::ALL, Self::id, setting)
    }

    pub fn stacked(self) -> bool {
        self != Self::SideBySide
    }
}

fn from_setting<T: Copy + Default>(all: &[T], id: fn(T) -> &'static str, setting: &str) -> T {
    all.iter().copied().find(|s| id(*s).eq_ignore_ascii_case(setting.trim())).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tabs {
    #[default]
    Sidebar,
    Top,
}

impl Tabs {
    pub const ALL: [Self; 2] = [Self::Sidebar, Self::Top];

    pub fn id(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Top => "top",
        }
    }

    pub fn choices() -> Vec<(&'static str, &'static str)> {
        let note = |t: Self| match t {
            Self::Sidebar => "under their workspace in the sidebar",
            Self::Top => "in a bar above the pane, only the active workspace's",
        };
        Self::ALL.into_iter().map(|t| (t.id(), note(t))).collect()
    }

    pub fn from_setting(setting: &str) -> Self {
        from_setting(&Self::ALL, Self::id, setting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Widths {
    pub projects: u16,
    pub workspaces: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<u16>,
}

impl Default for Widths {
    fn default() -> Self {
        Self { projects: SIDEBAR_WIDTH, workspaces: WORKSPACES_WIDTH, changes: None, stack: None, agents: None }
    }
}

fn columns_room(total: u16) -> u16 {
    total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH)
}

impl Widths {
    #[must_use]
    pub fn fit(self, total: u16) -> Self {
        let room = columns_room(total);
        let workspaces = self.workspaces.min(room.saturating_sub(self.projects)).max(MIN_COLUMN_WIDTH);
        let projects = self.projects.min(room.saturating_sub(workspaces)).max(MIN_COLUMN_WIDTH);
        Self { projects, workspaces, ..self }
    }

    pub fn changes_width(self, total: u16) -> u16 {
        let max = total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH);
        let half = total.saturating_sub(self.projects + self.workspaces + PANE_PADDING) / 2;
        self.changes.unwrap_or_else(|| half.min(changes::DEFAULT_WIDTH)).max(changes::MIN_WIDTH).min(max)
    }

    pub fn main_width(self, total: u16, changes: bool) -> u16 {
        if changes { total.saturating_sub(self.changes_width(total)) } else { total }
    }

    #[must_use]
    pub fn dragged(self, border: Border, x: u16, total: u16) -> Self {
        let fitted = self.fit(total);
        let room = columns_room(total);
        let right = x.saturating_add(1);
        match border {
            Border::Projects => {
                let max = room.saturating_sub(fitted.workspaces).max(MIN_COLUMN_WIDTH);
                Self { projects: right.clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Workspaces => {
                let max = room.saturating_sub(fitted.projects).max(MIN_COLUMN_WIDTH);
                Self { workspaces: right.saturating_sub(fitted.projects).clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Changes => {
                let max = total.saturating_sub(PANE_PADDING + MIN_PANE_WIDTH + 2 * MIN_COLUMN_WIDTH);
                Self { changes: Some(total.saturating_sub(x).clamp(changes::MIN_WIDTH.min(max), max)), ..self }
            }
            Border::Stack | Border::Agents => self,
        }
    }

    #[must_use]
    pub fn reset(self, border: Border) -> Self {
        match border {
            Border::Projects => Self { projects: SIDEBAR_WIDTH, ..self },
            Border::Workspaces => Self { workspaces: WORKSPACES_WIDTH, ..self },
            Border::Stack => Self { stack: None, ..self },
            Border::Agents => Self { agents: None, ..self },
            Border::Changes => Self { changes: None, ..self },
        }
    }

    pub fn stacked_width(self, total: u16) -> u16 {
        self.projects.min(columns_room(total)).max(MIN_COLUMN_WIDTH)
    }

    pub fn top_rows(self, room: u16) -> u16 {
        if room < 2 * MIN_STACK_SECTION {
            return room / 2;
        }
        self.stack.unwrap_or(room / 2).clamp(MIN_STACK_SECTION, room - MIN_STACK_SECTION)
    }

    pub fn agents_rows(self, room: u16, keep: u16) -> u16 {
        if room < keep + MIN_STACK_SECTION {
            return room / 3;
        }
        self.agents.unwrap_or(room / 3).clamp(MIN_STACK_SECTION, room - keep)
    }

    #[must_use]
    pub fn stacked_dragged(self, border: Border, pos: Position, area: Rect, agents: bool) -> Self {
        match border {
            Border::Projects => {
                let max = columns_room(area.width).max(MIN_COLUMN_WIDTH);
                Self { projects: pos.x.saturating_add(1).clamp(MIN_COLUMN_WIDTH, max), ..self }
            }
            Border::Stack => {
                let mut room = stack_room(area);
                if agents {
                    let [rest, ..] = split_agents(Rect { height: room.height + 1, ..room }, self, STACKED_KEEP);
                    room.height = rest.height.saturating_sub(1);
                }
                let wanted = Self { stack: Some(pos.y.saturating_sub(room.y)), ..self };
                Self { stack: Some(wanted.top_rows(room.height)), ..self }
            }
            Border::Workspaces | Border::Agents | Border::Changes => self,
        }
    }
}

const STACKED_KEEP: u16 = 2 * MIN_STACK_SECTION + 1;

fn split_agents(r: Rect, widths: Widths, keep: u16) -> [Rect; 3] {
    let room = r.height.saturating_sub(1);
    let rows = widths.agents_rows(room, keep);
    let rest = Rect { height: room - rows, ..r };
    let line = Rect { y: rest.bottom(), height: r.height.min(1), ..r };
    let agents = Rect { y: line.bottom(), height: rows, ..r };
    [rest, line, agents]
}

fn stack_room(column: Rect) -> Rect {
    let y = column.y.saturating_add(HEADER_HEIGHT).min(column.bottom());
    Rect { y, height: column.bottom().saturating_sub(y).saturating_sub(STACK_FOOTER + 1), ..column }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Projects,
    Workspaces,
    Agents,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Areas {
    pub pitch: u16,
    pub tree: bool,
    pub bar: Rect,
    pub search: Rect,
    pub search_button: Rect,
    pub back: Rect,
    pub sidebar: Rect,
    pub title: Rect,
    pub list: Rect,
    pub separator: Rect,
    pub settings: Rect,
    pub usage: Rect,
    pub quit: Rect,
    pub workspaces: Rect,
    pub workspaces_title: Rect,
    pub workspaces_list: Rect,
    pub workspaces_separator: Rect,
    pub issues: Rect,
    pub results: Rect,
    pub pane: Rect,
    pub projects_border: Rect,
    pub workspaces_border: Rect,
    pub stack_border: Rect,
    pub agents: Rect,
    pub agents_title: Rect,
    pub agents_list: Rect,
    pub agents_border: Rect,
    pub agents_button: Rect,
    pub changes: Rect,
    pub changes_border: Rect,
    pub changes_button: Rect,
    pub todo_button: Rect,
    pub files_button: Rect,
    pub tab_bar: Rect,
}

impl Areas {
    pub fn border(&self, border: Border) -> Rect {
        match border {
            Border::Projects => self.projects_border,
            Border::Workspaces => self.workspaces_border,
            Border::Stack => self.stack_border,
            Border::Agents => self.agents_border,
            Border::Changes => self.changes_border,
        }
    }

    pub fn border_hit(&self, pos: Position) -> Option<Border> {
        [Border::Projects, Border::Workspaces, Border::Stack, Border::Agents, Border::Changes]
            .into_iter()
            .find(|&b| self.border(b).contains(pos))
    }

    pub fn compact(&self) -> bool {
        !self.bar.is_empty()
    }

    pub fn todo_dot(&self) -> Option<Rect> {
        let r = self.todo_button;
        let x = r.x.checked_sub(1).filter(|&x| !self.compact() && x > self.files_button.right())?;
        Some(Rect { x, width: 1, ..r })
    }

    #[must_use]
    pub fn with_tab_bar(self, shown: bool) -> Self {
        if !shown || self.compact() {
            return self;
        }
        let height = tab_bar::HEIGHT.min(self.pane.height);
        let pane = Rect { y: self.pane.y + height, height: self.pane.height - height, ..self.pane };
        Self { tab_bar: Rect { height, ..self.pane }, pane, ..self }
    }

    #[must_use]
    pub fn shown(self, nav: Option<Nav>) -> Self {
        if !self.compact() {
            return self;
        }
        let hidden = Rect::default();
        let projects = if nav == Some(Nav::Projects) {
            self
        } else {
            Self {
                sidebar: hidden,
                title: hidden,
                list: hidden,
                separator: hidden,
                settings: hidden,
                usage: hidden,
                quit: hidden,
                agents_button: hidden,
                ..self
            }
        };
        let workspaces = if nav == Some(Nav::Workspaces) {
            projects
        } else {
            Self {
                workspaces: hidden,
                workspaces_title: hidden,
                workspaces_list: hidden,
                workspaces_separator: hidden,
                issues: hidden,
                back: if nav == Some(Nav::Agents) { projects.back } else { hidden },
                ..projects
            }
        };
        if nav == Some(Nav::Agents) {
            workspaces
        } else {
            Self { agents: hidden, agents_title: hidden, agents_list: hidden, ..workspaces }
        }
    }
}

fn right_edge(r: Rect) -> Rect {
    Rect::new(r.right().saturating_sub(1), r.y, r.width.min(1), r.height)
}

fn sidebar_block() -> Block<'static> {
    Block::default().borders(Borders::RIGHT)
}

fn projects_column(r: Rect) -> [Rect; 6] {
    let [title, _, list, separator, settings, usage, quit] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(r);
    [title, list, separator, settings, usage, quit]
}

fn workspaces_column(r: Rect) -> [Rect; 5] {
    let [title, _, list, separator, issues, todo, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(r);
    [title, list, separator, issues, todo]
}

fn below_header(r: Rect) -> Rect {
    Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(0)]).areas::<2>(r)[1]
}

pub fn layout(area: Rect, widths: Widths) -> Areas {
    layout_with(area, widths, false, Sidebar::SideBySide)
}

pub fn layout_with(area: Rect, widths: Widths, changes: bool, sidebar: Sidebar) -> Areas {
    full_layout(area, widths, changes, sidebar, false)
}

pub fn full_layout(area: Rect, widths: Widths, changes: bool, sidebar: Sidebar, agents: bool) -> Areas {
    if area.width < COMPACT_WIDTH {
        return compact_layout(area, changes, agents);
    }
    let columns = |r: Rect| match sidebar {
        Sidebar::SideBySide => wide_layout(r, widths, agents),
        Sidebar::Tree => tree_layout(r, widths, agents),
        Sidebar::ProjectsOnTop | Sidebar::WorkspacesOnTop => stacked_layout(r, widths, sidebar, agents),
    };
    if !changes {
        return columns(area);
    }
    let [main, panel] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(widths.changes_width(area.width))]).areas(area);
    let border = Rect { width: panel.width.min(1), ..panel };
    let content = Rect { x: panel.x.saturating_add(1), width: panel.width.saturating_sub(1), ..panel };
    Areas { changes: content, changes_border: border, ..columns(main) }
}

fn search_area(header: Rect) -> Rect {
    Rect { height: header.height.min(1), ..header }.inner(Margin::new(1, 0))
}

fn agents_section(r: Rect, widths: Widths, keep: u16, agents: bool) -> (Rect, Areas) {
    if !agents {
        return (r, Areas::default());
    }
    let [rest, line, section_rect] = split_agents(r, widths, keep);
    let [agents_title, agents_list] = section(section_rect);
    let areas = Areas { agents: section_rect, agents_title, agents_list, agents_border: line, ..Areas::default() };
    (rest, areas)
}

fn wide_layout(area: Rect, widths: Widths, agents: bool) -> Areas {
    let Widths { projects, workspaces, .. } = widths.fit(area.width);
    let [columns, _, pane] = Layout::horizontal([
        Constraint::Length(projects + workspaces),
        Constraint::Length(PANE_PADDING),
        Constraint::Min(1),
    ])
    .areas(area);
    let [left, workspaces] =
        Layout::horizontal([Constraint::Length(projects), Constraint::Length(workspaces)]).areas(columns);
    let [header, results] = Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(0)])
        .areas(Rect { width: columns.width.saturating_sub(1), ..columns });
    let search = search_area(header);
    let sidebar = below_header(left);
    let [title, list, separator, settings, usage, quit] = projects_column(sidebar_block().inner(sidebar));
    let lists = Rect { height: list.bottom().saturating_sub(title.y), ..title };
    let (lists, agents_areas) = agents_section(lists, widths, MIN_STACK_SECTION, agents);
    let [title, list] = if agents { section(lists) } else { [title, list] };
    let [workspaces_title, workspaces_list, workspaces_separator, issues, todo] =
        workspaces_column(below_header(sidebar_block().inner(workspaces)));
    Areas {
        pitch: 1,
        tree: false,
        bar: Rect::default(),
        search,
        search_button: search,
        back: Rect::default(),
        sidebar,
        title,
        list,
        separator,
        settings,
        usage,
        quit,
        workspaces,
        workspaces_title,
        workspaces_list,
        workspaces_separator,
        issues,
        results,
        pane,
        projects_border: right_edge(sidebar),
        workspaces_border: right_edge(workspaces),
        stack_border: Rect::default(),
        changes: Rect::default(),
        changes_border: Rect::default(),
        changes_button: Rect::default(),
        todo_button: changes_button(todo, TODO_LABEL),
        files_button: files_button(todo),
        ..agents_areas
    }
}

fn section(r: Rect) -> [Rect; 2] {
    let [title, _, list] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(GAP), Constraint::Min(0)]).areas(r);
    [title, list]
}

fn one_column(area: Rect, widths: Widths, keep: u16, agents: bool) -> (Areas, Rect) {
    let column = Rect { width: widths.stacked_width(area.width), ..area };
    let pane_x = column.right().saturating_add(PANE_PADDING).min(area.right());
    let pane = Rect { x: pane_x, width: area.right() - pane_x, ..area };
    let inner = sidebar_block().inner(column);
    let [header, results] = Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(0)]).areas(inner);
    let search = search_area(header);
    let room = stack_room(inner);
    let footer_y = inner.bottom().saturating_sub(STACK_FOOTER).max(room.y);
    let footer = Rect { y: footer_y, height: inner.bottom() - footer_y, ..inner };
    let [separator, issues, todo, settings, usage, quit] = Layout::vertical([Constraint::Length(1); 6]).areas(footer);
    let frame = Areas {
        pitch: 1,
        search,
        search_button: search,
        sidebar: column,
        separator,
        settings,
        usage,
        quit,
        issues,
        results,
        pane,
        projects_border: right_edge(column),
        todo_button: changes_button(todo, TODO_LABEL),
        files_button: files_button(todo),
        ..Areas::default()
    };
    let (sections, section) = agents_section(Rect { height: footer_y - room.y, ..room }, widths, keep, agents);
    let frame = Areas {
        agents: Rect { x: column.x, width: column.width, ..section.agents },
        agents_title: section.agents_title,
        agents_list: section.agents_list,
        agents_border: section.agents_border,
        ..frame
    };
    (frame, sections)
}

fn stacked_layout(area: Rect, widths: Widths, sidebar: Sidebar, agents: bool) -> Areas {
    let (frame, sections) = one_column(area, widths, STACKED_KEEP, agents);
    let room = Rect { height: sections.height.saturating_sub(1), ..sections };
    let top_rows = widths.top_rows(room.height);
    let top = Rect { height: top_rows, ..room }.intersection(sections);
    let line = Rect { y: room.y + top_rows, height: 1, ..room }.intersection(sections);
    let under = Rect { y: room.y + top_rows + 1, height: room.height - top_rows, ..room }.intersection(sections);
    let (projects, workspaces) = if sidebar == Sidebar::WorkspacesOnTop { (under, top) } else { (top, under) };
    let [title, list] = section(projects);
    let [workspaces_title, workspaces_list] = section(workspaces);
    let widen = |r: Rect| Rect { x: frame.sidebar.x, width: frame.sidebar.width, ..r };
    Areas {
        sidebar: widen(projects),
        title,
        list,
        workspaces: widen(workspaces),
        workspaces_title,
        workspaces_list,
        stack_border: line,
        ..frame
    }
}

fn tree_layout(area: Rect, widths: Widths, agents: bool) -> Areas {
    let (frame, sections) = one_column(area, widths, MIN_STACK_SECTION, agents);
    let [title, list] = section(sections);
    Areas {
        tree: true,
        sidebar: Rect { x: frame.sidebar.x, width: frame.sidebar.width, ..sections },
        title,
        list,
        ..frame
    }
}

fn compact_layout(area: Rect, changes: bool, agents: bool) -> Areas {
    let pitch = COMPACT_PITCH;
    let [bar, below] = Layout::vertical([Constraint::Length(pitch), Constraint::Min(0)]).areas(area);
    let search_width = COMPACT_BUTTON_WIDTH.min(bar.width);
    let search_button = Rect { x: bar.right() - search_width, width: search_width, ..bar };
    let todo_width = COMPACT_BUTTON_WIDTH.min(search_button.x.saturating_sub(bar.x));
    let todo_button = Rect { x: search_button.x - todo_width, width: todo_width, ..bar };
    let files_width = COMPACT_BUTTON_WIDTH.min(todo_button.x.saturating_sub(bar.x));
    let files_button = Rect { x: todo_button.x - files_width, width: files_width, ..bar };
    let changes_width = COMPACT_BUTTON_WIDTH.min(files_button.x.saturating_sub(bar.x));
    let changes_button = Rect { x: files_button.x - changes_width, width: changes_width, ..bar };
    let [_, menu] = Layout::vertical([Constraint::Length(GAP), Constraint::Min(0)]).areas(below);
    let column = |footer: u16| {
        Layout::vertical([
            Constraint::Length(pitch),
            Constraint::Length(GAP),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(footer),
        ])
        .areas::<5>(menu)
    };
    let [title, _, list, separator, footer] = column(pitch);
    let [settings, usage, quit] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).areas(footer);
    let [workspaces_title, _, workspaces_list, workspaces_separator, issues] = column(pitch);
    let back = Rect { width: button_width(BACK_LABEL) + 2, ..workspaces_title }.intersection(workspaces_title);
    let [agents_title, _, agents_list] =
        Layout::vertical([Constraint::Length(pitch), Constraint::Length(GAP), Constraint::Min(1)]).areas(menu);
    let agents_width = (button_width(AGENTS_LABEL) + 2).min(title.width);
    let agents_button = Rect { x: title.right() - agents_width, width: agents_width, ..title };
    let shown = |r: Rect| if agents { r } else { Rect::default() };
    Areas {
        pitch,
        tree: false,
        bar,
        search: bar,
        search_button,
        back,
        sidebar: below,
        title,
        list,
        separator,
        settings,
        usage,
        quit,
        workspaces: below,
        workspaces_title,
        workspaces_list,
        workspaces_separator,
        issues,
        results: below,
        pane: below,
        projects_border: Rect::default(),
        workspaces_border: Rect::default(),
        stack_border: Rect::default(),
        agents: shown(below),
        agents_title: shown(agents_title),
        agents_list: shown(agents_list),
        agents_border: Rect::default(),
        agents_button: shown(agents_button),
        changes: if changes { below } else { Rect::default() },
        changes_border: Rect::default(),
        changes_button,
        todo_button,
        files_button,
        tab_bar: Rect::default(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    list: Rect,
    heights: Vec<u16>,
    button: u16,
    scroll: usize,
}

impl Rows {
    fn room(&self) -> u16 {
        self.list.height.saturating_sub(GAP + self.button)
    }

    fn height(&self, range: Range<usize>) -> u32 {
        self.heights[range].iter().map(|&h| u32::from(h)).sum()
    }

    fn fitting_before(&self, end: usize) -> usize {
        let room = u32::from(self.room());
        let mut start = end;
        while start > 0 && self.height(start - 1..end) <= room {
            start -= 1;
        }
        start
    }

    fn first(&self) -> usize {
        self.scroll.min(self.fitting_before(self.heights.len()))
    }

    fn end(&self) -> usize {
        let (first, room) = (self.first(), u32::from(self.room()));
        (first..self.heights.len()).take_while(|&i| self.height(first..i + 1) <= room).last().map_or(first, |i| i + 1)
    }

    pub fn item(&self, i: usize) -> Rect {
        let first = self.first();
        if !(first..self.end()).contains(&i) {
            return Rect::default();
        }
        let y = u32::from(self.list.y) + self.height(first..i);
        Rect::new(self.list.x, u16::try_from(y).unwrap_or(u16::MAX), self.list.width, self.heights[i])
    }

    pub fn at(&self, pos: Position) -> Option<usize> {
        (self.first()..self.end()).find(|&i| self.item(i).contains(pos))
    }

    pub fn hidden(&self) -> (Range<usize>, Range<usize>) {
        (0..self.first(), self.end()..self.heights.len())
    }

    pub fn scrolled(&self, delta: isize) -> usize {
        let max = self.fitting_before(self.heights.len());
        self.scroll.min(max).saturating_add_signed(delta).min(max)
    }

    pub fn reveal(&self, i: usize) -> usize {
        let first = self.first();
        if i >= self.heights.len() || (first..self.end()).contains(&i) {
            first
        } else if i < first {
            i
        } else {
            self.fitting_before(i + 1).min(i)
        }
    }

    pub fn button(&self) -> Rect {
        let gap = if self.heights.is_empty() { 0 } else { u32::from(GAP) };
        let below = u32::from(self.list.y) + self.height(0..self.heights.len()) + gap;
        let last = self.list.bottom().saturating_sub(self.button);
        let y = u16::try_from(below).unwrap_or(u16::MAX).min(last);
        Rect::new(self.list.x, y, self.list.width, self.button)
    }

    pub fn more_below(&self) -> Rect {
        Rect::new(self.list.x, self.list.y.saturating_add(self.room()), self.list.width, 1).intersection(self.list)
    }

    pub fn edge(&self, pos: Position) -> Option<isize> {
        let (above, below) = self.hidden();
        let y = self.list.y.saturating_add(self.room());
        let bottom = Rect { y, height: self.list.bottom().saturating_sub(y), ..self.list };
        if !above.is_empty() && more_above(self.list).contains(pos) {
            Some(-1)
        } else if !below.is_empty() && bottom.contains(pos) {
            Some(1)
        } else {
            None
        }
    }

    fn zone(&self, pos: Position) -> Option<Zone> {
        if more_above(self.list).contains(pos) {
            return Some(Zone::Above);
        }
        if !self.list.contains(pos) {
            return None;
        }
        Some(self.at(pos).map_or(Zone::Below, Zone::Row))
    }

    fn boundary(&self, zone: Zone, on_row: impl Fn(usize) -> Option<usize>) -> Option<usize> {
        match zone {
            Zone::Above => Some(self.first()),
            Zone::Row(i) => on_row(i),
            Zone::Below => Some(self.end()),
        }
    }
}

pub fn more_above(list: Rect) -> Rect {
    if list.y < GAP { Rect::default() } else { Rect::new(list.x, list.y - GAP, list.width, 1) }
}

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

pub fn active_row(rows: &[SidebarRow], project: usize, group: Option<usize>) -> Option<usize> {
    let find = |row: SidebarRow| rows.iter().position(|r| *r == row);
    find(SidebarRow::Project(project)).or_else(|| find(SidebarRow::Group(group?)))
}

fn row_rect<R: PartialEq>(layout: &Rows, rows: &[R], row: &R) -> Rect {
    rows.iter().position(|r| r == row).map_or_else(Rect::default, |i| layout.item(i))
}

pub fn project_rows(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize) -> Rows {
    let heights = rows.iter().map(|r| if matches!(r, SidebarRow::Gap | SidebarRow::Landing) { GAP } else { pitch });
    Rows { list, heights: heights.collect(), button: pitch, scroll }
}

pub fn entry_row(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize, row: SidebarRow) -> Rect {
    row_rect(&project_rows(list, pitch, rows, scroll), rows, &row)
}

pub fn close_button(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize, row: SidebarRow) -> Rect {
    row_close_button(entry_row(list, pitch, rows, scroll, row), pitch)
}

pub fn new_project_button(list: Rect, pitch: u16, rows: &[SidebarRow]) -> Rect {
    project_rows(list, pitch, rows, 0).button()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarHit {
    Select(usize),
    Close(usize),
    Menu(usize),
    Group(usize),
    CloseGroup(usize),
    GroupMenu(usize),
    New,
}

pub fn sidebar_hit(list: Rect, pitch: u16, rows: &[SidebarRow], scroll: usize, pos: Position) -> Option<SidebarHit> {
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

pub fn row_close_button(row: Rect, pitch: u16) -> Rect {
    let width = if pitch > 1 { COMPACT_CLOSE_WIDTH } else { CLOSE_BUTTON_WIDTH };
    Rect::new(row.right().saturating_sub(width), row.y, width.min(row.width), pitch.min(row.height))
}

pub fn row_menu_button(row: Rect, pitch: u16) -> Rect {
    let close = row_close_button(row, pitch);
    let width = match pitch {
        _ if row.width < MIN_MENU_ROW_WIDTH => 0,
        1 => MENU_BUTTON_WIDTH,
        _ => COMPACT_MENU_WIDTH,
    };
    let x = close.x.saturating_sub(width).max(row.x);
    Rect::new(x, row.y, close.x - x, close.height)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowButton {
    Close,
    Menu,
}

fn row_button(row: Rect, pitch: u16, pos: Position) -> Option<RowButton> {
    if row_close_button(row, pitch).contains(pos) {
        Some(RowButton::Close)
    } else if row_menu_button(row, pitch).contains(pos) {
        Some(RowButton::Menu)
    } else {
        None
    }
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

pub fn agent_rows(list: Rect, pitch: u16, agents: usize, scroll: usize) -> Rows {
    Rows { list, heights: vec![pitch.max(AGENT_LINES); agents], button: 0, scroll }
}

pub fn agent_row(list: Rect, pitch: u16, agents: usize, scroll: usize, i: usize) -> Rect {
    agent_rows(list, pitch, agents, scroll).item(i)
}

pub fn agent_hit(list: Rect, pitch: u16, agents: usize, scroll: usize, pos: Position) -> Option<usize> {
    agent_rows(list, pitch, agents, scroll).at(pos)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeShape {
    pub groups: Vec<bool>,
    pub projects: Vec<ProjectShape>,
    pub tab_bar: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectShape {
    pub group: Option<usize>,
    pub collapsed: bool,
    pub workspaces: Vec<WorkspaceShape>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceShape {
    pub collapsed: bool,
    pub tabs: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeRow {
    Gap,
    Group(usize),
    Project(usize),
    Workspace(usize, usize),
    Tab(usize, usize, usize),
    NewTab(usize, usize),
    NewWorkspace(usize),
    Landing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeHit {
    Fold(TreeRow),
    Select(TreeRow),
    Close(TreeRow),
    Menu(TreeRow),
    NewTab(usize, usize),
    NewWorkspace(usize),
    NewProject,
}

impl TreeRow {
    fn workspace(self) -> Option<(usize, usize)> {
        match self {
            Self::Workspace(p, w) | Self::Tab(p, w, _) => Some((p, w)),
            _ => None,
        }
    }
}

impl TreeHit {
    pub fn workspace(self) -> Option<(usize, usize)> {
        match self {
            Self::Fold(row) | Self::Select(row) | Self::Close(row) | Self::Menu(row) => row.workspace(),
            Self::NewTab(p, w) => Some((p, w)),
            Self::NewWorkspace(_) | Self::NewProject => None,
        }
    }
}

pub fn tree_rows(shape: &TreeShape) -> Vec<TreeRow> {
    let groups: Vec<Option<usize>> = shape.projects.iter().map(|p| p.group).collect();
    let mut rows = Vec::new();
    for row in sidebar_rows(&groups, &shape.groups) {
        match row {
            SidebarRow::Gap => rows.push(TreeRow::Gap),
            SidebarRow::Group(g) => rows.push(TreeRow::Group(g)),
            SidebarRow::Project(p) => {
                rows.push(TreeRow::Project(p));
                let project = &shape.projects[p];
                if project.collapsed {
                    continue;
                }
                for (w, workspace) in project.workspaces.iter().enumerate() {
                    rows.push(TreeRow::Workspace(p, w));
                    if !workspace.collapsed && !shape.tab_bar {
                        rows.extend((0..workspace.tabs.len()).map(|t| TreeRow::Tab(p, w, t)));
                        rows.push(TreeRow::NewTab(p, w));
                    }
                }
                rows.push(TreeRow::NewWorkspace(p));
            }
            SidebarRow::Landing => {}
        }
    }
    rows
}

fn tree_height(shape: &TreeShape, row: TreeRow) -> u16 {
    match row {
        TreeRow::Gap | TreeRow::Landing => GAP,
        TreeRow::Tab(p, w, t) => {
            let lines = shape.projects.get(p).and_then(|p| p.workspaces.get(w)).and_then(|w| w.tabs.get(t));
            lines.copied().unwrap_or(1).max(1)
        }
        _ => 1,
    }
}

fn tree_depth(shape: &TreeShape, row: TreeRow) -> u16 {
    let project = |p: usize| u16::from(shape.projects.get(p).is_some_and(|p| p.group.is_some()));
    match row {
        TreeRow::Gap | TreeRow::Landing | TreeRow::Group(_) => 0,
        TreeRow::Project(p) => project(p),
        TreeRow::Workspace(p, _) | TreeRow::NewWorkspace(p) => project(p) + 1,
        TreeRow::Tab(p, ..) | TreeRow::NewTab(p, _) => project(p) + 2,
    }
}

fn tree_indent(shape: &TreeShape, row: TreeRow) -> u16 {
    2 + 2 * tree_depth(shape, row)
}

fn arrow_in(r: Rect, shape: &TreeShape, row: TreeRow) -> Rect {
    match row {
        TreeRow::Workspace(..) if shape.tab_bar => Rect::default(),
        TreeRow::Group(_) | TreeRow::Project(_) | TreeRow::Workspace(..) => {
            Rect { x: r.x.saturating_add(tree_indent(shape, row)), width: 2, height: 1, ..r }.intersection(r)
        }
        _ => Rect::default(),
    }
}

pub fn tree_layout_rows(list: Rect, shape: &TreeShape, rows: &[TreeRow], scroll: usize) -> Rows {
    Rows { list, heights: rows.iter().map(|r| tree_height(shape, *r)).collect(), button: 1, scroll }
}

pub fn tree_row(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    let rows = tree_rows(shape);
    row_rect(&tree_layout_rows(list, shape, &rows, scroll), &rows, &row)
}

pub fn tree_arrow(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    arrow_in(tree_row(list, shape, scroll, row), shape, row)
}

pub fn tree_close(list: Rect, shape: &TreeShape, scroll: usize, row: TreeRow) -> Rect {
    row_close_button(tree_row(list, shape, scroll, row), 1)
}

pub fn tree_drop(list: Rect, shape: &TreeShape, scroll: usize, dragged: TreeRow, pos: Position) -> Option<Landing> {
    let rows = tree_rows(shape);
    let layout = tree_layout_rows(list, shape, &rows, scroll);
    let zone = layout.zone(pos)?;
    match dragged {
        TreeRow::Group(_) => {
            let header = |r| if let TreeRow::Group(g) = r { Some(g) } else { None };
            let (at, g) = block_drop(&rows, &layout, zone, dragged, TreeRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Group(g) })
        }
        TreeRow::Project(_) => tree_project_drop(&rows, &layout, zone, rows.iter().position(|r| *r == dragged)?),
        TreeRow::Workspace(p, w) => tree_workspace_drop(&rows, &layout, zone, p, w),
        TreeRow::Tab(p, w, t) => {
            let tab = |r| if let TreeRow::Tab(q, v, u) = r { (q == p && v == w).then_some(u) } else { None };
            sibling_drop(&rows, &layout, zone, (TreeRow::Workspace(p, w), TreeRow::NewTab(p, w)), tab, t)
        }
        _ => None,
    }
}

pub fn tree_active_row(rows: &[TreeRow], shape: &TreeShape, active: (usize, usize, Option<usize>)) -> Option<usize> {
    let (p, w, t) = active;
    let find = |row: TreeRow| rows.iter().position(|r| *r == row);
    t.and_then(|t| find(TreeRow::Tab(p, w, t)))
        .or_else(|| find(TreeRow::Workspace(p, w)))
        .or_else(|| find(TreeRow::Project(p)))
        .or_else(|| find(TreeRow::Group(shape.projects.get(p)?.group?)))
}

pub fn tree_hit(list: Rect, shape: &TreeShape, scroll: usize, pos: Position) -> Option<TreeHit> {
    if !list.contains(pos) {
        return None;
    }
    let rows = tree_rows(shape);
    let layout = tree_layout_rows(list, shape, &rows, scroll);
    if layout.button().contains(pos) {
        return Some(TreeHit::NewProject);
    }
    let i = layout.at(pos)?;
    let (r, row) = (layout.item(i), rows[i]);
    let button = row_button(r, 1, pos);
    Some(match row {
        TreeRow::Gap | TreeRow::Landing => return None,
        TreeRow::NewTab(p, w) => TreeHit::NewTab(p, w),
        TreeRow::NewWorkspace(p) => TreeHit::NewWorkspace(p),
        _ if button == Some(RowButton::Close) => TreeHit::Close(row),
        _ if button == Some(RowButton::Menu) => TreeHit::Menu(row),
        TreeRow::Group(_) => TreeHit::Fold(row),
        _ if arrow_in(r, shape, row).contains(pos) => TreeHit::Fold(row),
        _ => TreeHit::Select(row),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Group(usize),
    Project { group: Option<usize>, before: Option<usize> },
    Workspace(usize),
    Tab(usize),
}

impl Spot {
    fn indent(self) -> u16 {
        match self {
            Self::Project { group: Some(_), .. } | Self::Tab(_) => 4,
            Self::Group(_) | Self::Project { group: None, .. } | Self::Workspace(_) => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub at: usize,
    pub spot: Spot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    Sidebar(SidebarRow, Option<Landing>),
    Workspaces(WorkspaceRow, Option<Landing>),
    Tree(TreeRow, Option<Landing>),
    Bar(usize, Option<usize>),
    Todo(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    Above,
    Row(usize),
    Below,
}

trait ListRow: Copy + PartialEq {
    const GAP: Self;
    fn group(self) -> Option<usize>;
    fn project(self) -> Option<usize>;
}

impl ListRow for SidebarRow {
    const GAP: Self = Self::Gap;

    fn group(self) -> Option<usize> {
        if let Self::Group(g) = self { Some(g) } else { None }
    }

    fn project(self) -> Option<usize> {
        if let Self::Project(p) = self { Some(p) } else { None }
    }
}

impl ListRow for TreeRow {
    const GAP: Self = Self::Gap;

    fn group(self) -> Option<usize> {
        if let Self::Group(g) = self { Some(g) } else { None }
    }

    fn project(self) -> Option<usize> {
        if let Self::Project(p) = self { Some(p) } else { None }
    }
}

fn group_of<R: ListRow>(rows: &[R], i: usize) -> Option<usize> {
    rows[..=i].iter().rev().find_map(|r| r.group())
}

fn project_spot<R: ListRow>(rows: &[R], dragged: Range<usize>, at: usize) -> Spot {
    if let Some(j) = (at..rows.len()).find(|j| !dragged.contains(j))
        && let Some(q) = rows[j].project()
    {
        return Spot::Project { group: group_of(rows, j), before: Some(q) };
    }
    let group = match (0..at).rev().find(|k| !dragged.contains(k)) {
        Some(k) if rows[k] == R::GAP => group_of(rows, k - 1),
        Some(k) => group_of(rows, k),
        None => None,
    };
    Spot::Project { group, before: None }
}

fn project_drop(rows: &[SidebarRow], layout: &Rows, zone: Zone, dragged: usize) -> Option<Landing> {
    let at = layout.boundary(zone, |i| match rows[i] {
        _ if i == dragged => None,
        SidebarRow::Project(_) if i < dragged => Some(i),
        SidebarRow::Project(_) | SidebarRow::Group(_) => Some(i + 1),
        SidebarRow::Gap | SidebarRow::Landing => Some(i),
    })?;
    Some(Landing { at, spot: project_spot(rows, dragged..dragged + 1, at) })
}

fn block_drop<R: Copy + PartialEq>(
    rows: &[R],
    layout: &Rows,
    zone: Zone,
    dragged: R,
    gap: R,
    header: impl Fn(R) -> Option<usize>,
) -> Option<(usize, usize)> {
    let d = rows.iter().position(|r| *r == dragged)?;
    let len = rows.len();
    let is_header = |j: usize| header(rows[j]).is_some();
    let block = |i: usize| (0..=i).rev().take_while(|&j| rows[j] != gap).find(|&j| is_header(j));
    let at = layout.boundary(zone, |i| match block(i) {
        Some(h) if h == d => None,
        Some(h) if h < d => Some(h),
        Some(h) => Some((h..len).find(|&j| rows[j] == gap).unwrap_or(len)),
        None => Some((i..len).find(|&j| is_header(j)).unwrap_or(len)),
    })?;
    let count = rows.iter().filter(|r| header(**r).is_some()).count();
    Some((at, rows[at..].iter().find_map(|r| header(*r)).unwrap_or(count)))
}

fn sibling_drop<R: Copy + PartialEq>(
    rows: &[R],
    layout: &Rows,
    zone: Zone,
    (header, end): (R, R),
    tab: impl Fn(R) -> Option<usize>,
    t: usize,
) -> Option<Landing> {
    let header = rows.iter().position(|r| *r == header)?;
    let end = rows.iter().position(|r| *r == end)?;
    let at = layout.boundary(zone, |i| match tab(rows[i]) {
        Some(u) if u == t => None,
        Some(u) if u < t => Some(i),
        Some(_) => Some(i + 1),
        None if i <= header => Some(header + 1),
        None => Some(end),
    })?;
    let at = at.clamp(header + 1, end);
    Some(Landing { at, spot: Spot::Tab(at - header - 1) })
}

fn tab_drop(rows: &[WorkspaceRow], layout: &Rows, zone: Zone, w: usize, t: usize) -> Option<Landing> {
    let tab = |r| if let WorkspaceRow::Tab(v, u) = r { (v == w).then_some(u) } else { None };
    sibling_drop(rows, layout, zone, (WorkspaceRow::Workspace(w), WorkspaceRow::NewTab(w)), tab, t)
}

fn inside(parent: TreeRow, row: TreeRow) -> bool {
    match (parent, row) {
        (
            TreeRow::Project(p),
            TreeRow::Workspace(q, _) | TreeRow::Tab(q, ..) | TreeRow::NewTab(q, _) | TreeRow::NewWorkspace(q),
        ) => p == q,
        (TreeRow::Workspace(p, w), TreeRow::Tab(q, v, _) | TreeRow::NewTab(q, v)) => p == q && w == v,
        _ => false,
    }
}

fn block_end(rows: &[TreeRow], i: usize) -> usize {
    (i + 1..rows.len()).find(|&j| !inside(rows[i], rows[j])).unwrap_or(rows.len())
}

fn owner(rows: &[TreeRow], i: usize, is_owner: impl Fn(TreeRow) -> bool) -> usize {
    (0..=i).rev().find(|&j| is_owner(rows[j]) && (j == i || inside(rows[j], rows[i]))).unwrap_or(i)
}

fn block_drop_at(
    rows: &[TreeRow],
    layout: &Rows,
    zone: Zone,
    d: usize,
    is_owner: impl Fn(TreeRow) -> bool,
) -> Option<usize> {
    let dragged = d..block_end(rows, d);
    let at = layout.boundary(zone, |i| {
        let o = owner(rows, i, &is_owner);
        match rows[o] {
            _ if dragged.contains(&o) => None,
            row if is_owner(row) && o < d => Some(o),
            row if is_owner(row) => Some(block_end(rows, o)),
            TreeRow::Group(_) => Some(o + 1),
            _ => Some(o),
        }
    })?;
    let o = owner(rows, at.min(rows.len().saturating_sub(1)), &is_owner);
    Some(if at < rows.len() && o < at && is_owner(rows[o]) { block_end(rows, o) } else { at })
}

fn tree_project_drop(rows: &[TreeRow], layout: &Rows, zone: Zone, d: usize) -> Option<Landing> {
    let at = block_drop_at(rows, layout, zone, d, |r| matches!(r, TreeRow::Project(_)))?;
    Some(Landing { at, spot: project_spot(rows, d..block_end(rows, d), at) })
}

fn tree_workspace_drop(rows: &[TreeRow], layout: &Rows, zone: Zone, p: usize, w: usize) -> Option<Landing> {
    let header = rows.iter().position(|r| *r == TreeRow::Project(p))?;
    let end = rows.iter().position(|r| *r == TreeRow::NewWorkspace(p))?;
    let d = rows.iter().position(|r| *r == TreeRow::Workspace(p, w))?;
    let is_workspace = |r| matches!(r, TreeRow::Workspace(q, _) if q == p);
    let at = block_drop_at(rows, layout, zone, d, is_workspace)?.clamp(header + 1, end);
    let index = |r: &TreeRow| if let TreeRow::Workspace(q, v) = *r { (q == p).then_some(v) } else { None };
    let before =
        rows[at..end].iter().find_map(index).unwrap_or_else(|| rows[header..end].iter().filter_map(index).count());
    Some(Landing { at, spot: Spot::Workspace(before) })
}

pub fn sidebar_drop(
    list: Rect,
    pitch: u16,
    rows: &[SidebarRow],
    scroll: usize,
    dragged: SidebarRow,
    pos: Position,
) -> Option<Landing> {
    let layout = project_rows(list, pitch, rows, scroll);
    let zone = layout.zone(pos)?;
    match dragged {
        SidebarRow::Project(_) => project_drop(rows, &layout, zone, rows.iter().position(|r| *r == dragged)?),
        SidebarRow::Group(_) => {
            let header = |r| if let SidebarRow::Group(g) = r { Some(g) } else { None };
            let (at, g) = block_drop(rows, &layout, zone, dragged, SidebarRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Group(g) })
        }
        SidebarRow::Gap | SidebarRow::Landing => None,
    }
}

pub fn workspace_drop(
    list: Rect,
    pitch: u16,
    tabs: &TabLines,
    scroll: usize,
    dragged: WorkspaceRow,
    pos: Position,
) -> Option<Landing> {
    let layout = workspace_layout(list, pitch, tabs, scroll);
    let zone = layout.zone(pos)?;
    let rows = workspace_rows(tabs);
    match dragged {
        WorkspaceRow::Workspace(_) => {
            let header = |r| if let WorkspaceRow::Workspace(w) = r { Some(w) } else { None };
            let (at, w) = block_drop(&rows, &layout, zone, dragged, WorkspaceRow::Gap, header)?;
            Some(Landing { at, spot: Spot::Workspace(w) })
        }
        WorkspaceRow::Tab(w, t) => tab_drop(&rows, &layout, zone, w, t),
        WorkspaceRow::Gap | WorkspaceRow::NewTab(_) | WorkspaceRow::Landing => None,
    }
}

fn landed<R: Copy + PartialEq>(
    rows: Vec<R>,
    layout: &Rows,
    landing: Option<Landing>,
    gap: R,
    mark: R,
) -> (Vec<R>, Rect) {
    let (first, end) = (layout.first(), layout.end());
    let Some(at) = landing.map(|l| l.at).filter(|at| (first..=end).contains(at)) else {
        return (rows, Rect::default());
    };
    let mut drawn = rows;
    let beside = [at.checked_sub(1).filter(|&g| g >= first), Some(at).filter(|&g| g < end)];
    if let Some(g) = beside.into_iter().flatten().find(|&g| drawn[g] == gap) {
        drawn[g] = mark;
        return (drawn, Rect::default());
    }
    if at == first {
        return (drawn, more_above(layout.list));
    }
    if at == end {
        let last = layout.item(end - 1);
        return (drawn, Rect { y: last.bottom(), height: 1, ..last });
    }
    drawn.insert(at, mark);
    (drawn, Rect::default())
}

pub fn menu_area(area: Rect, at: Position, items: &[impl AsRef<str>]) -> Rect {
    let longest = items.iter().map(|item| item.as_ref().chars().count()).max().unwrap_or(0);
    let width = u16::try_from(longest).unwrap_or(u16::MAX).saturating_add(4).min(area.width);
    let height = u16::try_from(items.len()).unwrap_or(u16::MAX).saturating_add(2).min(area.height);
    let x = at.x.min(area.right().saturating_sub(width));
    let y = at.y.saturating_add(1).min(area.bottom().saturating_sub(height));
    Rect::new(x, y, width, height)
}

pub fn menu_item(menu: Rect, i: usize) -> Rect {
    let y = menu.y.saturating_add(1).saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
    Rect::new(menu.x + 1, y, menu.width.saturating_sub(2), 1).intersection(menu)
}

pub fn menu_hit(menu: Rect, items: usize, pos: Position) -> Option<usize> {
    (0..items).find(|&i| menu_item(menu, i).contains(pos))
}

pub fn form_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(FORM_WIDTH);
    let height = FORM_HEIGHT.min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn form_inner(form: Rect) -> Rect {
    Block::bordered().inner(form).inner(Margin::new(FORM_PADDING, 0))
}

fn form_rows(form: Rect) -> [Rect; 6] {
    let [label, input, hint, toggle, note, _, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(form_inner(form));
    [label, input, hint, toggle, note, buttons]
}

pub fn form_toggle(form: Rect) -> Rect {
    form_rows(form)[3]
}

fn button_width(label: &str) -> u16 {
    u16::try_from(label.chars().count()).unwrap_or(u16::MAX).saturating_add(2)
}

pub fn changes_button(issues: Rect, label: &str) -> Rect {
    let width = button_width(label);
    Rect { x: issues.right().saturating_sub(width + 1), width, ..issues }.intersection(issues)
}

pub fn files_button(row: Rect) -> Rect {
    Rect { x: row.x.saturating_add(1), width: button_width(files::LABEL), ..row }.intersection(row)
}

pub fn update_button(settings: Rect, label: &str) -> Rect {
    let width = button_width(label).min(settings.width);
    Rect { x: settings.right() - width, width, ..settings }
}

pub fn form_buttons(form: Rect, submit: &str) -> [Rect; 2] {
    buttons_in(form_rows(form)[5], submit, CANCEL_LABEL)
}

fn buttons_in(row: Rect, submit: &str, cancel: &str) -> [Rect; 2] {
    let cancel_width = button_width(cancel);
    let submit_width = button_width(submit);
    let cancel = Rect::new(row.right().saturating_sub(cancel_width), row.y, cancel_width, 1).intersection(row);
    let submit = Rect::new(cancel.x.saturating_sub(submit_width + 1), row.y, submit_width, 1).intersection(row);
    [submit, cancel]
}

pub fn form_extra_button(form: Rect, submit: &str, extra: &str) -> Rect {
    let row = form_rows(form)[5];
    let [submit, _] = form_buttons(form, submit);
    let right = submit.x.saturating_sub(1).max(row.x);
    let x = right.saturating_sub(button_width(extra)).max(row.x);
    Rect { x, width: right - x, ..row }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormHit {
    Submit,
    Extra,
    Cancel,
    Toggle,
}

pub fn form_hit(area: Rect, submit: &str, extra: Option<&str>, pos: Position) -> Option<FormHit> {
    let form = form_area(area);
    let [s, c] = form_buttons(form, submit);
    if s.contains(pos) {
        Some(FormHit::Submit)
    } else if extra.is_some_and(|extra| form_extra_button(form, submit, extra).contains(pos)) {
        Some(FormHit::Extra)
    } else if c.contains(pos) {
        Some(FormHit::Cancel)
    } else if form_toggle(form).contains(pos) {
        Some(FormHit::Toggle)
    } else {
        None
    }
}

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

pub fn style_icon(area: Rect, i: usize) -> Rect {
    grid_cell(style_rows(area)[1], ICONS_PER_ROW, ICON_CELL, i)
}

pub fn style_colour(area: Rect, i: usize) -> Rect {
    grid_cell(style_rows(area)[3], COLOURS_PER_ROW, COLOUR_CELL, i)
}

pub fn style_done(area: Rect) -> Rect {
    update_button(style_rows(area)[4], crate::settings::DONE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleHit {
    Icon(usize),
    Colour(usize),
    Done,
}

pub fn style_hit(area: Rect, pos: Position) -> Option<StyleHit> {
    let [_, icons, _, colours, last] = style_rows(area);
    if update_button(last, crate::settings::DONE).contains(pos) {
        return Some(StyleHit::Done);
    }
    let cell = |grid, per_row, width, count| (0..count).find(|&i| grid_cell(grid, per_row, width, i).contains(pos));
    cell(icons, ICONS_PER_ROW, ICON_CELL, GROUP_ICONS.len())
        .map(StyleHit::Icon)
        .or_else(|| cell(colours, COLOURS_PER_ROW, COLOUR_CELL, GROUP_COLOURS.len()).map(StyleHit::Colour))
}

pub fn picker_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(PICKER_WIDTH);
    let height = area.height.saturating_sub(2).min(PICKER_HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn picker_rows(picker: Rect) -> [Rect; 4] {
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

fn update_rows(update: Rect) -> [Rect; 4] {
    let [message, _, notes, note, buttons] = Layout::vertical([
        Constraint::Length(UPDATE_MESSAGE_HEIGHT),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(update));
    [message, notes, note, buttons]
}

pub fn update_notes(area: Rect) -> Rect {
    update_rows(picker_area(area))[1]
}

pub fn update_buttons(area: Rect, submit: &str, cancel: &str) -> [Rect; 2] {
    buttons_in(update_rows(picker_area(area))[3], submit, cancel)
}

pub fn update_scroll(area: Rect, lines: usize, scroll: usize) -> usize {
    scroll.min(lines.saturating_sub(usize::from(update_notes(area).height)))
}

pub fn usage_area(area: Rect, usage: &Usage) -> Rect {
    let width = if area.width < 49 { area.width } else { area.width.saturating_sub(2) }.min(FORM_WIDTH);
    let body = usage_inner(Rect::new(0, 0, width, 3)).width;
    let lines = u16::try_from(
        usage_lines((Color::Reset, Color::Reset), usage, body)
            .len()
            .max(stats::lines((Color::Reset, Color::Reset, false), &usage.stats, body).len()),
    )
    .unwrap_or(u16::MAX);
    let height = lines.saturating_add(5).min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn usage_inner(usage: Rect) -> Rect {
    form_inner(usage)
}

fn usage_rows(usage: Rect) -> [Rect; 3] {
    let [tabs, _, body, buttons] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(usage_inner(usage));
    [tabs, body, buttons]
}

pub fn usage_tabs(area: Rect, usage: &Usage) -> Vec<Rect> {
    tabs_in(usage_rows(usage_area(area, usage))[0], &crate::usage::Page::NAMES)
}
pub fn usage_share(area: Rect, usage: &Usage) -> Rect {
    buttons_in(usage_rows(usage_area(area, usage))[2], "share", crate::settings::DONE)[0]
}

pub fn usage_done(area: Rect, usage: &Usage) -> Rect {
    update_button(usage_rows(usage_area(area, usage))[2], crate::settings::DONE)
}

pub fn picker_list(picker: Rect) -> Rect {
    picker_rows(picker)[1]
}

pub fn picker_buttons(picker: Rect, submit: &str) -> [Rect; 2] {
    buttons_in(picker_rows(picker)[3], submit, CANCEL_LABEL)
}

fn first_visible(list: Rect, items: usize, scroll: usize) -> usize {
    scroll.min(items.saturating_sub(usize::from(list.height)))
}

pub fn picker_item(picker: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    list_item(picker_list(picker), items, scroll, i)
}

pub fn list_item_at(list_box: Rect, items: usize, scroll: usize, i: usize) -> Position {
    list_item(picker_list(list_box), items, scroll, i).as_position()
}

fn list_item(list: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    let Some(row) = i.checked_sub(first_visible(list, items, scroll)).filter(|_| i < items) else {
        return Rect::default();
    };
    let y = list.y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
    Rect::new(list.x, y, list.width, 1).intersection(list)
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

pub fn issues_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(4).min(ISSUES_WIDTH);
    let height = area.height.saturating_sub(2).min(ISSUES_HEIGHT);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

pub fn settings_area(area: Rect) -> Rect {
    issues_area(area)
}

fn settings_rows(settings: Rect) -> [Rect; 5] {
    let [tabs, _, body, edit, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(settings));
    [tabs, body, edit, note, buttons]
}

pub fn settings_tabs(settings: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(settings_rows(settings)[0], names)
}

pub fn settings_body(settings: Rect) -> Rect {
    settings_rows(settings)[1]
}

pub fn settings_edit(settings: Rect) -> Rect {
    settings_rows(settings)[2]
}

pub fn settings_done(settings: Rect) -> Rect {
    settings_buttons(settings)[1]
}

pub fn settings_restart(settings: Rect) -> Rect {
    settings_buttons(settings)[0]
}

fn settings_buttons(settings: Rect) -> [Rect; 2] {
    let rects = issue_buttons_in(settings_rows(settings)[4], &[crate::settings::RESTART, crate::settings::DONE]);
    [rects[0], rects[1]]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsLine {
    Blank,
    Header(usize),
    Row(usize),
}

pub fn settings_lines(sections: &[&str]) -> Vec<SettingsLine> {
    let mut lines = Vec::new();
    for (i, section) in sections.iter().enumerate() {
        if !section.is_empty() && (i == 0 || sections[i - 1] != *section) {
            if i > 0 {
                lines.push(SettingsLine::Blank);
            }
            lines.push(SettingsLine::Header(i));
        }
        lines.push(SettingsLine::Row(i));
    }
    lines
}

pub fn settings_scroll(sections: &[&str], cursor: usize, height: usize) -> usize {
    let lines = settings_lines(sections);
    let at = lines.iter().position(|l| *l == SettingsLine::Row(cursor)).unwrap_or(0);
    let header = at.saturating_sub(1);
    if at < height { 0 } else { (at + 1).saturating_sub(height).min(header) }
}

pub fn settings_row(area: Rect, sections: &[&str], cursor: usize, row: usize) -> Rect {
    let body = settings_body(settings_area(area));
    let scroll = settings_scroll(sections, cursor, usize::from(body.height));
    let Some(line) = settings_lines(sections).iter().position(|l| *l == SettingsLine::Row(row)) else {
        return Rect::default();
    };
    let Some(visible) = line.checked_sub(scroll) else { return Rect::default() };
    let y = body.y.saturating_add(u16::try_from(visible).unwrap_or(u16::MAX));
    Rect::new(body.x, y, body.width, 1).intersection(body)
}

pub fn settings_remove(row: Rect) -> Rect {
    row_close_button(row, 1)
}

pub fn settings_moves(row: Rect) -> [Rect; 2] {
    let down = Rect::new(row.right().saturating_sub(3), row.y, 3, 1).intersection(row);
    let up = Rect::new(row.right().saturating_sub(6), row.y, 3, 1).intersection(row);
    [up, down]
}

pub fn settings_pick_list(settings: Rect) -> Rect {
    let body = settings_body(settings);
    Rect::new(body.x, body.y.saturating_add(2), body.width, body.height.saturating_sub(2))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHit {
    Tab(usize),
    Row(usize),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    Pick(usize),
    Done,
    Restart,
}

pub struct SettingsLayout<'a> {
    pub tabs: &'a [&'a str],
    pub sections: &'a [&'a str],
    pub removable: &'a [bool],
    pub movable: &'a [bool],
    pub cursor: usize,
    pub pick: Option<(usize, usize)>,
}

pub fn settings_hit(area: Rect, layout: &SettingsLayout, pos: Position) -> Option<SettingsHit> {
    let settings = settings_area(area);
    if settings_done(settings).contains(pos) {
        return Some(SettingsHit::Done);
    }
    if settings_restart(settings).contains(pos) {
        return Some(SettingsHit::Restart);
    }
    if let Some(i) = settings_tabs(settings, layout.tabs).iter().position(|r| r.contains(pos)) {
        return Some(SettingsHit::Tab(i));
    }
    if let Some((items, scroll)) = layout.pick {
        let list = settings_pick_list(settings);
        if !list.contains(pos) {
            return None;
        }
        let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
        return (i < items).then_some(SettingsHit::Pick(i));
    }
    let i =
        (0..layout.sections.len()).find(|&i| settings_row(area, layout.sections, layout.cursor, i).contains(pos))?;
    let row = settings_row(area, layout.sections, layout.cursor, i);
    if layout.removable.get(i).copied().unwrap_or(false) && settings_remove(row).contains(pos) {
        return Some(SettingsHit::Remove(i));
    }
    if layout.movable.get(i).copied().unwrap_or(false) {
        let [up, down] = settings_moves(row);
        if up.contains(pos) {
            return Some(SettingsHit::MoveUp(i));
        }
        if down.contains(pos) {
            return Some(SettingsHit::MoveDown(i));
        }
    }
    Some(SettingsHit::Row(i))
}

fn issues_rows(issues: Rect) -> [Rect; 5] {
    let [tabs, _, input, _, list, note, buttons] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(form_inner(issues));
    [tabs, input, list, note, buttons]
}

pub fn issues_list(issues: Rect) -> Rect {
    issues_rows(issues)[2]
}

pub fn issue_detail(issues: Rect) -> Rect {
    let [tabs, _, _, note, _] = issues_rows(issues);
    Rect::new(tabs.x, tabs.y, tabs.width, note.y.saturating_sub(tabs.y))
}

fn row_of(row: Rect, x: u16, width: u16) -> Rect {
    Rect::new(x, row.y, width, 1).intersection(row)
}

pub fn issue_tabs(issues: Rect, names: &[&str]) -> Vec<Rect> {
    tabs_in(issues_rows(issues)[0], names)
}

fn tabs_in(row: Rect, names: &[&str]) -> Vec<Rect> {
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

fn toggle_width(label: &str) -> u16 {
    button_width(label).saturating_add(4)
}

pub fn issue_toggles(issues: Rect, labels: &[&str]) -> Vec<Rect> {
    right_aligned(issues_rows(issues)[0], labels, toggle_width, 2)
}

pub fn issue_buttons(issues: Rect, labels: &[&str]) -> Vec<Rect> {
    issue_buttons_in(issues_rows(issues)[4], labels)
}

fn issue_buttons_in(row: Rect, labels: &[&str]) -> Vec<Rect> {
    right_aligned(row, labels, button_width, 1)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssuesHit {
    Tab(usize),
    Toggle(usize),
    Item(usize),
    Button(usize),
}

pub fn issue_button_hit(area: Rect, buttons: &[&str], pos: Position) -> Option<usize> {
    issue_buttons(issues_area(area), buttons).iter().position(|r| r.contains(pos))
}

pub fn issues_hit(
    area: Rect,
    tabs: &[&str],
    toggles: &[&str],
    buttons: &[&str],
    items: usize,
    scroll: usize,
    pos: Position,
) -> Option<IssuesHit> {
    let issues = issues_area(area);
    if let Some(i) = issue_tabs(issues, tabs).iter().position(|r| r.contains(pos)) {
        return Some(IssuesHit::Tab(i));
    }
    if let Some(i) = issue_toggles(issues, toggles).iter().position(|r| r.contains(pos)) {
        return Some(IssuesHit::Toggle(i));
    }
    if let Some(i) = issue_button_hit(area, buttons, pos) {
        return Some(IssuesHit::Button(i));
    }
    let list = issues_list(issues);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(IssuesHit::Item(i))
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

fn results_rows(results: Rect) -> [Rect; 2] {
    let [list, _, hint] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(GAP), Constraint::Length(1)]).areas(results);
    [list, hint]
}

pub fn results_list(results: Rect) -> Rect {
    results_rows(results)[0]
}

pub fn result_item(results: Rect, items: usize, scroll: usize, i: usize) -> Rect {
    list_item(results_list(results), items, scroll, i)
}

pub fn result_hit(results: Rect, items: usize, scroll: usize, pos: Position) -> Option<usize> {
    let list = results_list(results);
    if !list.contains(pos) {
        return None;
    }
    let i = first_visible(list, items, scroll) + usize::from(pos.y - list.y);
    (i < items).then_some(i)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Error(String),
    Busy(&'static str),
}

pub struct Toggle {
    pub label: &'static str,
    pub on: bool,
}

pub struct Form {
    pub title: &'static str,
    pub label: &'static str,
    pub value: String,
    pub hint: String,
    pub toggle: Option<Toggle>,
    pub note: Option<Note>,
    pub submit: &'static str,
}

pub struct Confirm {
    pub title: &'static str,
    pub message: String,
    pub note: Option<Note>,
    pub submit: &'static str,
    pub extra: Option<&'static str>,
}

pub struct Update {
    pub title: &'static str,
    pub message: String,
    pub notes: Vec<Line<'static>>,
    pub scroll: usize,
    pub note: Option<Note>,
    pub submit: &'static str,
    pub cancel: &'static str,
}

pub struct UsageWindow {
    pub label: String,
    pub percent: u16,
    pub severity: Severity,
    pub resets: String,
}

pub struct UsageSection {
    pub title: String,
    pub status: String,
    pub error: Option<String>,
    pub windows: Vec<UsageWindow>,
    pub empty: Option<&'static str>,
    pub extra: Option<String>,
}

pub struct Usage {
    pub page: crate::usage::Page,
    pub stats: crate::stats::View,
    pub sections: Vec<UsageSection>,
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

pub struct SettingsRow {
    pub section: &'static str,
    pub label: String,
    pub value: String,
    pub note: String,
    pub dangerous: bool,
    pub removable: bool,
    pub movable: bool,
}

pub struct SettingsEdit {
    pub label: String,
    pub value: String,
}

pub struct SettingsPick {
    pub title: String,
    pub filter: String,
    pub items: Vec<(String, String, bool)>,
    pub selected: Option<usize>,
    pub scroll: usize,
}

pub struct Settings {
    pub tabs: Vec<&'static str>,
    pub tab: usize,
    pub rows: Vec<SettingsRow>,
    pub cursor: usize,
    pub edit: Option<SettingsEdit>,
    pub pick: Option<SettingsPick>,
    pub note: Option<Note>,
    pub hint: String,
    pub submit: &'static str,
}

impl Settings {
    pub fn sections(&self) -> Vec<&'static str> {
        self.rows.iter().map(|r| r.section).collect()
    }
}

pub struct IssueRow {
    pub key: String,
    pub title: String,
    pub meta: String,
}

pub enum IssuesBody {
    List { filter: String, items: Vec<IssueRow>, selected: Option<usize>, scroll: usize, empty: String },
    Token { label: &'static str, input: String, help: Vec<String> },
    Detail { lines: Vec<Line<'static>>, scroll: usize },
}

pub struct Issues {
    pub title: String,
    pub tabs: Vec<&'static str>,
    pub tab: usize,
    pub toggles: Vec<String>,
    pub on: Vec<bool>,
    pub body: IssuesBody,
    pub note: Option<Note>,
    pub hint: String,
    pub buttons: Vec<&'static str>,
}

pub struct ResultRow {
    pub name: String,
    pub context: String,
}

pub struct Search {
    pub query: String,
    pub results: Vec<ResultRow>,
    pub selected: usize,
    pub scroll: usize,
    pub hint: String,
}

impl Search {
    fn showing_results(&self) -> bool {
        !self.query.trim().is_empty()
    }
}

pub enum Overlay {
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
    pub fn is_modal(&self) -> bool {
        !matches!(self, Self::Menu { .. } | Self::Search(_) | Self::Keys(_))
    }

    pub fn area(&self, area: Rect) -> Option<Rect> {
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

pub struct Entry {
    pub name: String,
    pub branch: Option<String>,
}

pub struct ProjectEntry {
    pub name: String,
    pub workspaces: usize,
    pub group: Option<usize>,
    pub status: Option<Status>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupEntry {
    pub name: String,
    pub icon: char,
    pub colour: u8,
    #[serde(default)]
    pub collapsed: bool,
}

impl GroupEntry {
    pub fn label(&self) -> String {
        format!("{} {}", self.icon, self.name)
    }
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

pub struct TreeView {
    pub shape: TreeShape,
    pub workspaces: Vec<Vec<WorkspaceEntry>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub status: Option<Status>,
    pub agent: String,
    pub project: String,
    pub workspace: Option<String>,
    pub details: Details,
    pub active: bool,
}

pub struct AgentsView {
    pub entries: Vec<AgentEntry>,
    pub scroll: usize,
}

pub struct ChangesButton {
    pub label: String,
    pub open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastIcon {
    Check,
    Agent(Status),
    Bug,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastButton {
    Undo,
    Cancel,
}

impl ToastButton {
    fn label(self) -> &'static str {
        match self {
            Self::Undo => UNDO_LABEL,
            Self::Cancel => CANCEL_LABEL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toast<'a> {
    pub message: &'a str,
    pub icon: ToastIcon,
    pub button: Option<ToastButton>,
}

pub struct TabView {
    pub layout: Node<usize>,
    pub screens: Vec<Snapshot>,
    pub active: usize,
    pub dim_inactive: bool,
    pub dragging: Option<Vec<bool>>,
    pub link: Option<(u16, Range<u16>)>,
    pub landing: Option<(Rect, Place)>,
}

#[expect(clippy::struct_excessive_bools, reason = "each flag is a state or a setting the drawing follows")]
pub struct View<'a> {
    pub groups: Vec<GroupEntry>,
    pub projects: Vec<ProjectEntry>,
    pub active: usize,
    pub projects_scroll: usize,
    pub has_project: bool,
    pub workspaces: Vec<WorkspaceEntry>,
    pub active_workspace: usize,
    pub active_tab: Option<usize>,
    pub workspaces_scroll: usize,
    pub issues: bool,
    pub hover: Option<Position>,
    pub widths: Widths,
    pub sidebar: Sidebar,
    pub resizing: Option<Border>,
    pub light: bool,
    pub muted: Color,
    pub tab: Option<TabView>,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast<'a>>,
    pub nav: Option<Nav>,
    pub update: Option<String>,
    pub remote: Option<bool>,
    pub changes: Option<changes::View>,
    pub changes_button: Option<ChangesButton>,
    pub todo: Option<todo::View>,
    pub todo_dot: bool,
    pub todo_pending: bool,
    pub files: Option<files::View>,
    pub attention: Option<Status>,
    pub drag: Option<Drag>,
    pub tree: Option<TreeView>,
    pub agents: Option<AgentsView>,
    pub counts: bool,
    pub tab_bar: Option<tab_bar::TabBar>,
}

impl View<'_> {
    pub fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let groups: Vec<Option<usize>> = self.projects.iter().map(|p| p.group).collect();
        let collapsed: Vec<bool> = self.groups.iter().map(|g| g.collapsed).collect();
        sidebar_rows(&groups, &collapsed)
    }

    pub fn tab_lines(&self) -> TabLines {
        let lines = self.workspaces.iter().map(|w| w.tabs.iter().map(|t| t.details.lines()).collect()).collect();
        TabLines::new(lines, self.tab_bar.is_none())
    }

    fn surface(&self) -> Color {
        surface_colour(self.light)
    }

    fn hover_fill(&self) -> Color {
        hover_colour(self.light)
    }

    fn line(&self) -> Color {
        line_colour(self.light)
    }

    fn row_background(&self, r: Rect, active: bool) -> Style {
        if active {
            Style::default().bg(self.surface())
        } else if self.row_hovered(r) {
            Style::default().bg(self.hover_fill())
        } else {
            Style::default()
        }
    }

    fn row_hovered(&self, r: Rect) -> bool {
        self.resizing.is_none() && self.tab.as_ref().is_none_or(|t| t.dragging.is_none()) && sidebar_hovered(self, r)
    }

    fn sidebar_landing(&self) -> Option<Landing> {
        match self.drag {
            Some(Drag::Sidebar(_, landing)) => landing,
            _ => None,
        }
    }

    fn workspaces_landing(&self) -> Option<Landing> {
        match self.drag {
            Some(Drag::Workspaces(_, landing)) => landing,
            _ => None,
        }
    }

    fn dragging_entry(&self, row: SidebarRow) -> bool {
        matches!(self.drag, Some(Drag::Sidebar(dragged, _)) if dragged == row)
    }

    fn dragging_workspace_row(&self, row: WorkspaceRow) -> bool {
        matches!(self.drag, Some(Drag::Workspaces(dragged, _)) if dragged == row)
    }

    fn dragging_tree_row(&self, row: TreeRow) -> bool {
        matches!(self.drag, Some(Drag::Tree(dragged, _)) if dragged == row)
    }

    fn search(&self) -> Option<&Search> {
        match &self.overlay {
            Some(Overlay::Search(search)) => Some(search),
            _ => None,
        }
    }
}

pub fn draw(f: &mut Frame, view: &View) {
    let panel = view.changes.is_some() || view.todo.is_some() || view.files.is_some();
    let areas = full_layout(f.area(), view.widths, panel, view.sidebar, view.agents.is_some())
        .with_tab_bar(view.tab_bar.is_some())
        .shown(view.nav);
    if let Some(bar) = view.tab_bar.as_ref().filter(|_| view.has_project && !areas.tab_bar.is_empty()) {
        tab_bar::draw(f, view, bar, areas.tab_bar);
    }
    match &view.tab {
        Some(tab) => draw_tab(f, view, tab, areas.pane),
        None if view.has_project => draw_no_tab(f, view.muted, areas.pane),
        None if view.projects.is_empty() => draw_welcome(f, view.muted, areas.pane),
        None => {}
    }
    if areas.compact() {
        draw_bar(f, view, &areas);
        if view.nav.is_some() {
            f.render_widget(Clear, areas.pane);
        }
    } else {
        draw_column_border(f, view.line(), areas.projects_border);
        draw_column_border(f, view.line(), areas.workspaces_border);
        draw_separator(f, view.line(), areas.stack_border);
        draw_borders(f, view, &areas);
        draw_search_bar(f, view, areas.search);
    }
    if areas.tree {
        draw_tree(f, view, &areas);
    } else if !areas.sidebar.is_empty() {
        draw_sidebar(f, view, &areas);
    }
    if let Some(agents) = &view.agents {
        draw_agents(f, view, agents, &areas);
    }
    if [areas.separator, areas.settings, areas.usage, areas.quit].iter().any(|r| !r.is_empty()) {
        draw_footer(f, view, &areas);
    }
    if !areas.workspaces.is_empty() {
        draw_workspaces(f, view, &areas);
    }
    if view.has_project && [areas.workspaces_separator, areas.issues].iter().any(|r| !r.is_empty()) {
        draw_issues_row(f, view, &areas);
    }
    if panel && !areas.changes.is_empty() {
        f.render_widget(Clear, areas.changes);
        let border = areas.changes_border;
        draw_column_border(f, view.line(), border);
        draw_border(f, view, border, Border::Changes);
        let hover = view.hover.filter(|_| view.overlay.is_none());
        match (&view.changes, &view.todo, &view.files) {
            (Some(changes), _, _) => changes::draw(f, areas.changes, changes, hover.filter(|_| view.drag.is_none())),
            (None, Some(todo), _) => todo::draw(f, areas.changes, todo, hover),
            (None, None, Some(files)) => files::draw(f, areas.changes, files, hover),
            (None, None, None) => {}
        }
    }
    if view.overlay.as_ref().is_some_and(Overlay::is_modal) {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::default().add_modifier(Modifier::DIM));
    }
    match &view.overlay {
        Some(Overlay::Menu { at, items }) => draw_menu(f, view, *at, items),
        Some(Overlay::Form(form)) => draw_form(f, view, form),
        Some(Overlay::GroupStyle(group)) => draw_group_style(f, view, group),
        Some(Overlay::Confirm(confirm)) => draw_confirm(f, view, confirm),
        Some(Overlay::Update(update)) => draw_update(f, view, update),
        Some(Overlay::Usage(usage)) => draw_usage(f, view, usage),
        Some(Overlay::Remote(remote)) => remote_access::draw(f, view, remote),
        Some(Overlay::Picker(picker)) => draw_picker(f, view, picker),
        Some(Overlay::Issues(issues)) => draw_issues(f, view, issues),
        Some(Overlay::Settings(settings)) => draw_settings(f, view, settings),
        Some(Overlay::Search(search)) if search.showing_results() => draw_results(f, view, search, areas.results),
        Some(Overlay::Keys(menu)) => keys::draw(f, view, menu, areas.pane),
        Some(Overlay::Search(_)) | None => {}
    }
    if let Some(toast) = view.toast {
        draw_toast(f, view, toast);
    }
}

fn draw_centered(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let width = lines.iter().map(Line::width).max().unwrap_or(0);
    if height > area.height || width > usize::from(area.width) {
        return;
    }
    let r = Rect { y: area.y + (area.height - height) / 2, height, ..area };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), r);
}

fn draw_no_tab(f: &mut Frame, muted: Color, pane: Rect) {
    let dim = Style::default().fg(muted);
    draw_centered(
        f,
        pane,
        vec![
            Line::from(Span::styled(NO_TAB, dim.add_modifier(Modifier::BOLD))),
            Line::from(vec![Span::styled("+ Tab", Style::default().fg(Color::Cyan)), Span::styled(NO_TAB_HINT, dim)]),
        ],
    );
}

fn welcome_text(muted: Color) -> Vec<Line<'static>> {
    let dim = Style::default().fg(muted);
    vec![
        Line::from(wordmark()),
        Line::from(Span::styled(TAGLINE, dim)),
        Line::default(),
        Line::from(vec![Span::styled(NEW_BUTTON, Style::default().fg(Color::Cyan)), Span::styled(WELCOME_HINT, dim)]),
    ]
}

fn draw_welcome(f: &mut Frame, muted: Color, pane: Rect) {
    let corner = Style::default().fg(BRAND_COLOR);
    let cursor = Style::default().fg(Color::Cyan);
    let width = LOGO.iter().map(|(c, k)| c.chars().count() + k.chars().count()).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = LOGO
        .iter()
        .map(|(c, k)| {
            let pad = width - c.chars().count() - k.chars().count();
            Line::from(vec![Span::styled(*c, corner), Span::styled(*k, cursor), Span::raw(" ".repeat(pad))])
        })
        .collect();
    lines.push(Line::default());
    lines.extend(welcome_text(muted));
    if usize::from(pane.height) >= lines.len() {
        draw_centered(f, pane, lines);
    } else {
        draw_centered(f, pane, welcome_text(muted));
    }
}

fn wordmark() -> Vec<Span<'static>> {
    let mark = Style::default().fg(BRAND_COLOR).add_modifier(Modifier::BOLD);
    vec![Span::styled("c", mark), Span::styled("ornercase", Style::default().add_modifier(Modifier::BOLD))]
}

pub fn toast_area(area: Rect, toast: Toast) -> Rect {
    let button = toast.button.map_or(0, |button| button.label().chars().count() + 3);
    let text = TOAST_ICON.chars().count() + toast.message.chars().count() + 1 + button;
    let width = u16::try_from(text).unwrap_or(u16::MAX).saturating_add(2).min(area.width);
    let height = 3.min(area.height);
    let x = area.right().saturating_sub(width + TOAST_MARGIN).max(area.x);
    let y = area.bottom().saturating_sub(height + TOAST_MARGIN).max(area.y);
    Rect::new(x, y, width, height)
}

pub fn toast_button(area: Rect, toast: Toast) -> Rect {
    let Some(button) = toast.button else { return Rect::default() };
    let r = toast_area(area, toast).inner(Margin::new(1, 1));
    let width = button_width(button.label()).min(r.width);
    Rect { x: r.right().saturating_sub(width + 1), width, ..r }
}

pub fn scrollback_label(below: usize) -> String {
    format!(" ↓ {below} more ")
}

pub fn scrollback_badge(pane: Rect, below: usize) -> Option<Rect> {
    if below == 0 || pane.height == 0 {
        return None;
    }
    let width = u16::try_from(scrollback_label(below).chars().count()).unwrap_or(u16::MAX);
    if width.saturating_add(1) > pane.width {
        return None;
    }
    Some(Rect::new(pane.right() - width - 1, pane.bottom() - 1, width, 1))
}

fn draw_scrollback_badge(f: &mut Frame, view: &View, pane: Rect, below: usize) {
    let Some(badge) = scrollback_badge(pane, below) else { return };
    let lit = view.overlay.is_none() && view.hover.is_some_and(|p| badge.contains(p));
    let style = if lit {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(view.muted).bg(view.surface())
    };
    f.render_widget(Paragraph::new(Span::styled(scrollback_label(below), style)), badge);
}

fn draw_toast(f: &mut Frame, view: &View, toast: Toast) {
    let r = toast_area(f.area(), toast);
    let icon = match toast.icon {
        ToastIcon::Check => Span::styled(TOAST_ICON, Style::default().fg(Color::Green)),
        ToastIcon::Agent(status) => {
            let icon = status_icon(view.muted, status);
            Span::styled(format!(" {} ", icon.content), icon.style)
        }
        ToastIcon::Bug => Span::styled(BUG_ICON, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ToastIcon::Restart => Span::styled(RESTART_ICON, Style::default().fg(Color::Cyan)),
    };
    let border = Style::default().fg(icon.style.fg.unwrap_or(Color::Green));
    f.render_widget(Clear, r);
    f.render_widget(Block::bordered().border_type(BorderType::Rounded).border_style(border), r);
    f.render_widget(Paragraph::new(Line::from(vec![icon, Span::raw(toast.message)])), r.inner(Margin::new(1, 1)));
    let button = toast_button(f.area(), toast);
    if let Some(label) = toast.button.map(ToastButton::label).filter(|_| !button.is_empty()) {
        let lit = view.hover.is_some_and(|p| button.contains(p));
        let style = if lit {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        };
        f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), button);
    }
}

fn draw_tab(f: &mut Frame, view: &View, tab: &TabView, area: Rect) {
    let panes = tab.layout.visible(area, tab.active);
    let split = panes.len() > 1;
    for (i, pane) in panes {
        let Some(screen) = tab.screens.get(i) else { continue };
        let active = i == tab.active;
        draw_screen(f, screen, pane, active && view.overlay.is_none(), split && !active && tab.dim_inactive);
        if let Some((row, cols)) = tab.link.as_ref().filter(|_| active) {
            let r = Rect::new(pane.x + cols.start, pane.y + row, cols.end - cols.start, 1).intersection(pane);
            f.buffer_mut().set_style(r, Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED));
        }
        draw_scrollback_badge(f, view, pane, screen.below);
    }
    if let Some((r, place)) = tab.landing {
        draw_pane_landing(f, view, r, place);
    }
    draw_dividers(f, view, tab, area);
}

fn draw_pane_landing(f: &mut Frame, view: &View, r: Rect, place: Place) {
    f.buffer_mut().set_style(r, Style::default().bg(view.surface()));
    let label = format!(" {} ", if place == Place::Swap { SWAP_LABEL } else { MOVE_LABEL });
    let width = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
    if width > r.width || r.height == 0 {
        return;
    }
    let at = Rect::new(r.x + (r.width - width) / 2, r.y + r.height / 2, width, 1);
    let style = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(label, style)), at);
}

fn draw_screen(f: &mut Frame, screen: &Snapshot, pane: Rect, show_cursor: bool, dim: bool) {
    let buf = f.buffer_mut();
    for (y, row) in (pane.y..pane.bottom()).zip(&screen.rows) {
        for (x, cell) in (pane.x..pane.right()).zip(row) {
            let style = if dim { cell.style.add_modifier(Modifier::DIM) } else { cell.style };
            buf[(x, y)].set_symbol(&cell.symbol).set_style(style);
        }
    }
    if let Some(c) = screen.cursor.filter(|_| show_cursor)
        && c.x < pane.width
        && c.y < pane.height
    {
        f.set_cursor_position(Position::new(pane.x + c.x, pane.y + c.y));
    }
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

fn divider_symbol(links: u8) -> &'static str {
    match links {
        l if l == UP | DOWN | LEFT | RIGHT => "┼",
        l if l == UP | DOWN | RIGHT => "├",
        l if l == UP | DOWN | LEFT => "┤",
        l if l == LEFT | RIGHT | DOWN => "┬",
        l if l == LEFT | RIGHT | UP => "┴",
        l if l & (UP | DOWN) != 0 => "│",
        _ => "─",
    }
}

fn draw_dividers(f: &mut Frame, view: &View, tab: &TabView, area: Rect) {
    let dividers = tab.layout.dividers(area);
    let mut cells: HashMap<(u16, u16), (u8, bool)> = HashMap::new();
    for d in &dividers {
        let lit = tab.dragging.as_ref() == Some(&d.path) || (tab.dragging.is_none() && sidebar_hovered(view, d.line));
        let links = match d.dir {
            Dir::Right => UP | DOWN,
            Dir::Down => LEFT | RIGHT,
        };
        for y in d.line.top()..d.line.bottom() {
            for x in d.line.left()..d.line.right() {
                let cell = cells.entry((x, y)).or_default();
                cell.0 |= links;
                cell.1 |= lit;
            }
        }
    }
    let bridges: Vec<(u16, u16)> = dividers
        .iter()
        .filter(|d| d.dir == Dir::Down)
        .filter_map(|d| {
            let pad = d.line.x.checked_sub(1)?;
            let bar = pad.checked_sub(1)?;
            let joins =
                !cells.contains_key(&(pad, d.line.y)) && cells.get(&(bar, d.line.y)).is_some_and(|c| c.0 & UP != 0);
            joins.then_some((pad, d.line.y))
        })
        .collect();
    for (x, y) in bridges {
        cells.insert((x, y), (LEFT | RIGHT, false));
        if let Some(bar) = cells.get_mut(&(x - 1, y)) {
            bar.0 |= RIGHT;
        }
    }
    for d in &dividers {
        let Rect { x, y, .. } = d.line;
        let ends = match d.dir {
            Dir::Right => [(Some(x), y.checked_sub(1), DOWN), (Some(x), Some(d.line.bottom()), UP)],
            Dir::Down => [(x.checked_sub(1), Some(y), RIGHT), (Some(d.line.right()), Some(y), LEFT)],
        };
        for (x, y, link) in ends {
            if let (Some(x), Some(y)) = (x, y)
                && let Some(cell) = cells.get_mut(&(x, y))
            {
                cell.0 |= link;
            }
        }
    }
    let buf = f.buffer_mut();
    for ((x, y), (links, lit)) in cells {
        let color = if lit { Color::Cyan } else { view.line() };
        buf[(x, y)].set_symbol(divider_symbol(links)).set_style(Style::default().fg(color));
    }
}

fn hovered(view: &View, r: Rect) -> bool {
    view.hover.is_some_and(|p| r.contains(p))
}

fn dim(muted: Color) -> Style {
    Style::default().fg(muted)
}

fn hovered_at(hover: Option<Position>, r: Rect) -> bool {
    hover.is_some_and(|p| r.contains(p))
}

fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, end: u16) -> u16 {
    if x >= end {
        return x;
    }
    buf.set_stringn(x, y, text, usize::from(end - x), style).0
}

fn draw_close(buf: &mut Buffer, r: Rect, hover: Option<Position>, muted: Color) {
    let style =
        if hovered_at(hover, r) { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { dim(muted) };
    put(buf, r.x + 1, r.y, "×", style, r.right());
}

fn action_style(lit: bool) -> Style {
    if lit {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
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

fn rail(selected: bool, bg: Style) -> Span<'static> {
    if selected { Span::styled("▌", bg.fg(Color::Cyan)) } else { Span::styled(" ", bg) }
}

fn sidebar_hovered(view: &View, r: Rect) -> bool {
    view.overlay.is_none() && view.drag.is_none() && hovered(view, r)
}

fn overlay_block(muted: Color, title: &str) -> Block<'_> {
    let border = Style::default().fg(muted);
    let block = Block::bordered().border_type(BorderType::Rounded).border_style(border);
    if title.is_empty() {
        block
    } else {
        block.title(Line::from(vec![
            Span::styled("─", border),
            Span::styled(format!(" {title} "), Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD)),
        ]))
    }
}

fn draw_menu(f: &mut Frame, view: &View, at: Position, items: &[String]) {
    let menu = menu_area(f.area(), at, items);
    f.render_widget(Clear, menu);
    f.render_widget(overlay_block(view.muted, ""), menu);
    for (i, item) in items.iter().enumerate() {
        let r = menu_item(menu, i);
        let lit = hovered(view, r);
        let bg = if lit { Style::default().bg(view.surface()) } else { Style::default() };
        let text = if lit { bg.add_modifier(Modifier::BOLD) } else { bg };
        let line = Line::from(vec![rail(lit, bg), Span::styled(format!("{item} "), text)]);
        f.render_widget(Paragraph::new(line).style(bg), r);
    }
}

fn draw_form(f: &mut Frame, view: &View, form: &Form) {
    let r = form_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, form.title), r);
    let [label, input, hint, toggle, note, _] = form_rows(r);
    let dim = Style::default().fg(view.muted);

    f.render_widget(Paragraph::new(Span::styled(form.label, dim)), label);
    if let Some(t) = &form.toggle {
        let mark = if t.on { "[x] " } else { "[ ] " };
        let style = if hovered(view, toggle) { Style::default().fg(Color::Cyan) } else { Style::default() };
        f.render_widget(Paragraph::new(Span::styled(format!("{mark}{}", t.label), style)), toggle);
    }
    let max = usize::from(input.width).saturating_sub(INPUT_PROMPT.chars().count() + 1);
    let value = truncate_left(&form.value, max);
    let cursor_x = input.x + u16::try_from(INPUT_PROMPT.chars().count() + value.chars().count()).unwrap_or(0);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(INPUT_PROMPT, Style::default().fg(Color::Cyan)),
            Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
        ])),
        input,
    );
    f.render_widget(Paragraph::new(Span::styled(truncate_left(&form.hint, usize::from(hint.width)), dim)), hint);

    draw_note(f, view.muted, form.note.as_ref(), note);

    draw_dialog_buttons(f, view, form_buttons(r, form.submit), form.submit, CANCEL_LABEL);

    if !matches!(form.note, Some(Note::Busy(_))) && cursor_x < input.right() {
        f.set_cursor_position(Position::new(cursor_x, input.y));
    }
}

fn group_header(group: &GroupEntry, max: usize) -> Span<'static> {
    let style = Style::default().fg(Color::Indexed(group.colour)).add_modifier(Modifier::BOLD);
    Span::styled(format!("{} {}", group.icon, truncate_right(&group.name, max)), style)
}

fn draw_group_style(f: &mut Frame, view: &View, group: &GroupEntry) {
    let area = f.area();
    let r = form_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &group.name), r);
    let [icon_label, icons, colour_label, colours, last] = style_rows(area);
    let dim = Style::default().fg(view.muted);
    f.render_widget(Paragraph::new(Span::styled("icon", dim)), icon_label);
    f.render_widget(Paragraph::new(Span::styled("colour", dim)), colour_label);
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
    for (i, &colour) in GROUP_COLOURS.iter().enumerate() {
        let cell = grid_cell(colours, COLOURS_PER_ROW, COLOUR_CELL, i);
        let (open, close, edge) = if colour == group.colour {
            ("[", "]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
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
    let done = update_button(last, crate::settings::DONE);
    let max = usize::from(last.width.saturating_sub(done.width)).saturating_sub(5);
    let preview = Line::from(vec![Span::styled("▾ ", dim), group_header(group, max)]);
    f.render_widget(Paragraph::new(preview), last);
    draw_submit(f, view, done, crate::settings::DONE);
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

fn draw_confirm(f: &mut Frame, view: &View, confirm: &Confirm) {
    let r = form_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, confirm.title), r);
    let [label, _, _, toggle, note, _] = form_rows(r);
    let message = Rect::new(label.x, label.y, label.width, toggle.bottom().saturating_sub(label.y));
    f.render_widget(Paragraph::new(confirm.message.as_str()).wrap(Wrap { trim: true }), message);
    draw_note(f, view.muted, confirm.note.as_ref(), note);
    draw_dialog_buttons(f, view, form_buttons(r, confirm.submit), confirm.submit, CANCEL_LABEL);
    if let Some(extra) = confirm.extra {
        draw_secondary(f, view, form_extra_button(r, confirm.submit, extra), extra);
    }
}

fn draw_update(f: &mut Frame, view: &View, update: &Update) {
    let r = picker_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, update.title), r);
    let [message, notes, note, _] = update_rows(r);
    f.render_widget(Paragraph::new(update.message.as_str()).wrap(Wrap { trim: true }), message);
    let scroll = update_scroll(f.area(), update.notes.len(), update.scroll);
    let visible: Vec<Line> = update.notes.iter().skip(scroll).take(usize::from(notes.height)).cloned().collect();
    f.render_widget(Paragraph::new(visible), notes);
    draw_note(f, view.muted, update.note.as_ref(), note);
    draw_dialog_buttons(f, view, update_buttons(f.area(), update.submit, update.cancel), update.submit, update.cancel);
}

fn severity_color(severity: Severity) -> Color {
    match severity {
        Severity::Normal => Color::Green,
        Severity::Warning => WAITING_COLOR,
        Severity::Critical => Color::Red,
    }
}

fn usage_section_lines(
    (muted, track): (Color, Color),
    section: &UsageSection,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let dim = Style::default().fg(muted);
    let used = section.title.chars().count() + section.status.chars().count();
    lines.extend([
        Line::from(vec![
            Span::styled(section.title.clone(), Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" ".repeat(width.saturating_sub(used))),
            Span::styled(section.status.clone(), dim),
        ]),
        Line::default(),
    ]);
    if let Some(error) = &section.error {
        lines.extend(markdown::wrap_text(error, Style::default().fg(Color::Red), width));
        lines.push(Line::default());
    }
    for w in &section.windows {
        let colour = Style::default().fg(severity_color(w.severity));
        let percent = format!("{}%", w.percent);
        let resets = if w.resets.is_empty() { String::new() } else { format!(" · {}", w.resets) };
        let used = w.label.chars().count() + percent.chars().count() + resets.chars().count();
        let filled = (width * usize::from(w.percent.min(100))).div_ceil(100);
        lines.extend([
            Line::from(vec![
                Span::raw(w.label.clone()),
                Span::raw(" ".repeat(width.saturating_sub(used))),
                Span::styled(percent, colour.add_modifier(Modifier::BOLD)),
                Span::styled(resets, dim),
            ]),
            Line::from(vec![
                Span::styled(USAGE_BAR.repeat(filled), colour),
                Span::styled(USAGE_BAR.repeat(width - filled), Style::default().fg(track)),
            ]),
            Line::default(),
        ]);
    }
    if let Some(empty) = section.empty {
        lines.extend([Line::from(Span::styled(empty, dim)), Line::default()]);
    }
    if let Some(extra) = &section.extra {
        lines.extend([Line::from(extra.clone()), Line::default()]);
    }
}

fn usage_lines(colours: (Color, Color), usage: &Usage, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in &usage.sections {
        usage_section_lines(colours, section, usize::from(width), &mut lines);
    }
    lines
}

fn draw_usage(f: &mut Frame, view: &View, usage: &Usage) {
    let r = usage_area(f.area(), usage);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, USAGE_LABEL), r);
    let [_, body, _] = usage_rows(r);
    draw_tabs(f, view, &usage_tabs(f.area(), usage), &crate::usage::Page::NAMES, usage.page.index());
    let lines = if usage.page == crate::usage::Page::Stats {
        stats::lines((view.line(), view.muted, view.light), &usage.stats, body.width)
    } else {
        usage_lines((view.muted, view.line()), usage, body.width)
    };
    f.render_widget(Paragraph::new(lines), body);
    if usage.page == crate::usage::Page::Stats && !usage.stats.empty {
        draw_secondary(f, view, usage_share(f.area(), usage), "share");
    }
    draw_submit(f, view, usage_done(f.area(), usage), crate::settings::DONE);
}

fn dialog_button_style(view: &View, r: Rect, primary: bool) -> Style {
    match (primary, hovered(view, r)) {
        (true, true) => Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
        (true, false) => Style::default().fg(Color::Cyan).bg(view.surface()).add_modifier(Modifier::BOLD),
        (false, true) => Style::default().fg(Color::Black).bg(Color::Gray),
        (false, false) => Style::default().fg(Color::Gray).bg(view.surface()),
    }
}

fn draw_submit(f: &mut Frame, view: &View, r: Rect, label: &str) {
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

fn draw_picker(f: &mut Frame, view: &View, picker: &Picker) {
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

fn draw_settings(f: &mut Frame, view: &View, settings: &Settings) {
    let area = f.area();
    let r = settings_area(area);
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, "Settings"), r);
    let [_, body, edit_row, note, _] = settings_rows(r);
    let dim = Style::default().fg(view.muted);
    let mut cursor = None;
    draw_tabs(f, view, &settings_tabs(r, &settings.tabs), &settings.tabs, settings.tab);

    if let Some(pick) = &settings.pick {
        cursor = draw_input(f, view.muted, Rect { height: 1, ..body }, &pick.title, &pick.filter);
        let list = settings_pick_list(r);
        if pick.items.is_empty() {
            f.render_widget(Paragraph::new(Span::styled(" nothing matches", dim)), list);
        }
        for (i, (value, item_note, dangerous)) in pick.items.iter().enumerate() {
            let row = list_item(list, pick.items.len(), pick.scroll, i);
            if row.is_empty() {
                continue;
            }
            let selected = pick.selected == Some(i);
            let base = pick_background(view, selected, row);
            let value_style = if *dangerous { base.fg(Color::Red) } else { base };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    rail(selected, base),
                    Span::styled(format!("{value:<32} "), value_style.add_modifier(Modifier::BOLD)),
                    Span::styled(item_note.clone(), base.fg(view.muted)),
                ]))
                .style(base),
                row,
            );
        }
    } else {
        let sections = settings.sections();
        let scroll = settings_scroll(&sections, settings.cursor, usize::from(body.height));
        let lines = settings_lines(&sections);
        for (n, line) in lines.iter().skip(scroll).take(usize::from(body.height)).enumerate() {
            let y = body.y + u16::try_from(n).unwrap_or(0);
            let rect = Rect::new(body.x, y, body.width, 1);
            match line {
                SettingsLine::Blank => {}
                SettingsLine::Header(i) => f.render_widget(
                    Paragraph::new(Span::styled(settings.rows[*i].section, dim.add_modifier(Modifier::BOLD))),
                    rect,
                ),
                SettingsLine::Row(i) => draw_settings_row(f, view, &settings.rows[*i], *i == settings.cursor, rect),
            }
        }
    }

    if let Some(edit) = &settings.edit {
        cursor = draw_input(f, view.muted, edit_row, &format!("{}:", edit.label), &edit.value);
    }
    let (text, style) = match &settings.note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        None => (settings.hint.as_str(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(note.width)), style)), note);
    let buttons = [settings_done(r), settings_restart(r)];
    draw_dialog_buttons(f, view, buttons, settings.submit, crate::settings::RESTART);
    if let Some(pos) = cursor.filter(|_| !matches!(settings.note, Some(Note::Busy(_)))) {
        f.set_cursor_position(pos);
    }
}

fn draw_settings_row(f: &mut Frame, view: &View, row: &SettingsRow, selected: bool, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let (marker, bg) = if selected { ("▌ ", Style::default().bg(view.surface())) } else { ("  ", Style::default()) };
    let label_style = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
    let value_style = if row.dangerous { bg.fg(Color::Red) } else { bg };
    let width = usize::from(r.width);
    let label = truncate_right(&row.label, 24);
    let value = truncate_right(&row.value, width.saturating_sub(30) / 2 + 10);
    let spans = vec![
        Span::styled(marker, bg.fg(Color::Cyan)),
        Span::styled(format!("{label:<26}"), label_style),
        Span::styled(value.clone(), value_style),
        Span::styled(if value.is_empty() || row.note.is_empty() { String::new() } else { "  ".into() }, bg),
        Span::styled(row.note.clone(), bg.fg(view.muted)),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)).style(bg), r);
    if !hovered(view, r) {
        return;
    }
    if row.removable {
        let close = settings_remove(r);
        let style = if hovered(view, close) { bg.fg(Color::Red).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
        f.render_widget(Paragraph::new(Span::styled(" × ", style)), close);
    }
    if row.movable {
        for (rect, arrow) in settings_moves(r).into_iter().zip([" ↑ ", " ↓ "]) {
            let style =
                if hovered(view, rect) { bg.fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { bg.patch(dim) };
            f.render_widget(Paragraph::new(Span::styled(arrow, style)), rect);
        }
    }
}

fn draw_issues(f: &mut Frame, view: &View, issues: &Issues) {
    let r = issues_area(f.area());
    f.render_widget(Clear, r);
    f.render_widget(overlay_block(view.muted, &issues.title), r);
    let [_, input, list, note, _] = issues_rows(r);
    let dim = Style::default().fg(view.muted);

    let cursor = match &issues.body {
        IssuesBody::Detail { lines, scroll } => {
            let area = issue_detail(r);
            let visible: Vec<Line> = lines.iter().skip(*scroll).take(usize::from(area.height)).cloned().collect();
            f.render_widget(Paragraph::new(visible), area);
            None
        }
        IssuesBody::List { filter, items, selected, scroll, empty } => {
            draw_issue_tabs(f, view, issues, r);
            let cursor = draw_input(f, view.muted, input, "", filter);
            if items.is_empty() {
                f.render_widget(Paragraph::new(Span::styled(format!(" {empty}"), dim)), list);
            }
            draw_issue_rows(f, view, list, items, *selected, *scroll);
            cursor
        }
        IssuesBody::Token { label, input: typed, help } => {
            draw_issue_tabs(f, view, issues, r);
            let cursor = draw_input(f, view.muted, input, label, typed);
            let text: Vec<Line> = help.iter().map(|h| Line::from(h.as_str())).collect();
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), list.inner(Margin::new(1, 0)));
            cursor
        }
    };

    let (text, style) = match &issues.note {
        Some(Note::Error(text)) => (text.as_str(), Style::default().fg(Color::Red)),
        Some(Note::Busy(text)) => (*text, dim),
        None => (issues.hint.as_str(), dim),
    };
    f.render_widget(Paragraph::new(Span::styled(truncate_right(text, usize::from(note.width)), style)), note);

    let rects = issue_buttons(r, &issues.buttons);
    for (i, (rect, label)) in rects.iter().zip(&issues.buttons).enumerate() {
        let style = dialog_button_style(view, *rect, i == 0);
        f.render_widget(Paragraph::new(Span::styled(format!(" {label} "), style)), *rect);
    }

    if let Some(pos) = cursor.filter(|_| !matches!(issues.note, Some(Note::Busy(_)))) {
        f.set_cursor_position(pos);
    }
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

fn draw_issue_tabs(f: &mut Frame, view: &View, issues: &Issues, r: Rect) {
    draw_tabs(f, view, &issue_tabs(r, &issues.tabs), &issues.tabs, issues.tab);
    let labels: Vec<&str> = issues.toggles.iter().map(String::as_str).collect();
    for (rect, (label, on)) in issue_toggles(r, &labels).iter().zip(issues.toggles.iter().zip(&issues.on)) {
        let mark = if *on { "[x]" } else { "[ ]" };
        let style = if hovered(view, *rect) { Style::default().fg(Color::Cyan) } else { Style::default() };
        f.render_widget(Paragraph::new(Span::styled(format!(" {mark} {label} "), style)), *rect);
    }
}

fn tab_style(active: bool, lit: bool, surface: Color) -> Style {
    if active {
        Style::default().fg(Color::Cyan).bg(surface).add_modifier(Modifier::BOLD)
    } else if lit {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn draw_tabs(f: &mut Frame, view: &View, rects: &[Rect], names: &[&str], active: usize) {
    for (i, (rect, name)) in rects.iter().zip(names).enumerate() {
        let style = tab_style(i == active, hovered(view, *rect), view.surface());
        f.render_widget(Paragraph::new(Span::styled(format!(" {name} "), style)), *rect);
    }
}

fn draw_issue_rows(f: &mut Frame, view: &View, list: Rect, items: &[IssueRow], selected: Option<usize>, scroll: usize) {
    let key_width = items.iter().map(|i| i.key.chars().count()).max().unwrap_or(0);
    for (i, item) in items.iter().enumerate() {
        let row = list_item(list, items.len(), scroll, i);
        if row.is_empty() {
            continue;
        }
        let chosen = selected == Some(i);
        let bg = pick_background(view, chosen, row);
        let style = if chosen { bg.add_modifier(Modifier::BOLD) } else { bg };
        let width = usize::from(row.width);
        let meta = truncate_right(&item.meta, width / 3);
        let meta_width = meta.chars().count();
        let title = truncate_right(&item.title, width.saturating_sub(key_width + meta_width + 6));
        let gap = width.saturating_sub(key_width + title.chars().count() + meta_width + 5);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                rail(chosen, bg),
                Span::styled(format!("{:<key_width$}  ", item.key), style.fg(Color::Cyan)),
                Span::styled(format!("{title}{}", " ".repeat(gap)), style),
                Span::styled(format!("{meta} "), bg.fg(view.muted)),
            ]))
            .style(bg),
            row,
        );
    }
}

fn draw_sidebar(f: &mut Frame, view: &View, areas: &Areas) {
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

fn draw_hidden<R: Copy>(f: &mut Frame, muted: Color, layout: &Rows, rows: &[R], named: impl Fn(R) -> bool) {
    let (above, below) = layout.hidden();
    let count = |range: Range<usize>| (!range.is_empty()).then(|| rows[range].iter().filter(|r| named(**r)).count());
    draw_more(f, muted, [more_above(layout.list), layout.more_below()], count(above), count(below));
}

fn draw_new_project(f: &mut Frame, view: &View, r: Rect) {
    f.render_widget(Clear, r);
    draw_button(f, r, " ", NEW_BUTTON, button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan));
}

fn draw_landing(f: &mut Frame, r: Rect, indent: Option<u16>) {
    let Some(indent) = indent else { return };
    let row = Rect { height: r.height.min(1), ..r };
    let width = usize::from(row.width.saturating_sub(indent + 1));
    let line = Line::from(vec![Span::raw(" ".repeat(usize::from(indent))), Span::raw("─".repeat(width))]);
    f.render_widget(Paragraph::new(line.style(Style::default().fg(Color::Cyan))), row);
}

fn draw_footer(f: &mut Frame, view: &View, areas: &Areas) {
    draw_separator(f, view.line(), areas.separator);
    let mut settings = areas.settings;
    if let Some(label) = &view.update {
        let r = update_button(areas.settings, label);
        let idle = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        draw_button(f, r, "", label, button_style(view, r, idle, Color::Cyan));
        settings.width -= r.width;
    }
    draw_settings_button(f, view, settings);
    let mut usage = areas.usage;
    if let Some(connected) = view.remote {
        let r = remote_access::button(areas.usage, connected);
        remote_access::draw_button(f, view, r, connected);
        if !r.is_empty() {
            usage.width = r.x - usage.x;
        }
    }
    draw_usage_button(f, view, usage);
    draw_quit_button(f, view, areas.quit);
}

fn draw_column_border(f: &mut Frame, colour: Color, r: Rect) {
    let line = Paragraph::new(vec![Line::from("│"); usize::from(r.height)]).style(Style::default().fg(colour));
    f.render_widget(line, r);
}

fn draw_borders(f: &mut Frame, view: &View, areas: &Areas) {
    for border in [Border::Projects, Border::Workspaces, Border::Stack] {
        draw_border(f, view, areas.border(border), border);
    }
}

fn draw_border(f: &mut Frame, view: &View, r: Rect, border: Border) {
    if view.resizing == Some(border) || sidebar_hovered(view, r) {
        let buf = f.buffer_mut();
        for y in r.top()..r.bottom() {
            for x in r.left()..r.right() {
                buf[(x, y)].set_fg(Color::Cyan);
            }
        }
    }
}

fn draw_search_bar(f: &mut Frame, view: &View, r: Rect) {
    let dim = Style::default().fg(view.muted);
    let accent = Style::default().fg(Color::Cyan);
    let line = if let Some(search) = view.search() {
        let max = usize::from(r.width).saturating_sub(SEARCH_ICON.chars().count() + 1);
        let query = truncate_left(&search.query, max);
        let cursor = SEARCH_ICON.chars().count() + query.chars().count();
        if let Ok(offset) = u16::try_from(cursor)
            && offset < r.width
        {
            f.set_cursor_position(Position::new(r.x + offset, r.y));
        }
        Line::from(vec![
            Span::styled(SEARCH_ICON, accent),
            Span::styled(query, Style::default().add_modifier(Modifier::BOLD)),
        ])
    } else {
        let icon = if sidebar_hovered(view, r) { accent } else { dim };
        let room = usize::from(r.width).saturating_sub(SEARCH_ICON.chars().count() + 1);
        Line::from(vec![Span::styled(SEARCH_ICON, icon), Span::styled(truncate_right(SEARCH_PLACEHOLDER, room), dim)])
    };
    f.render_widget(Paragraph::new(line).style(Style::default().bg(view.surface())), r);
}

fn draw_results(f: &mut Frame, view: &View, search: &Search, r: Rect) {
    f.render_widget(Clear, r);
    let [list, hint] = results_rows(r);
    let dim = Style::default().fg(view.muted);
    if search.results.is_empty() {
        f.render_widget(Paragraph::new(Span::styled("  no matches", dim)), list);
    }
    for (i, result) in search.results.iter().enumerate() {
        let row = result_item(r, search.results.len(), search.scroll, i);
        if row.is_empty() {
            continue;
        }
        let selected = search.selected == i;
        let bg = pick_background(view, selected, row);
        let base = if selected { bg.add_modifier(Modifier::BOLD) } else { bg };
        let matched = base.fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let width = usize::from(row.width);
        let context = truncate_left(&result.context, width / 2);
        let context_width = context.chars().count();
        let name = truncate_right(&result.name, width.saturating_sub(context_width + 5));
        let gap = width.saturating_sub(name.chars().count() + context_width + 3);
        let mut spans = vec![rail(selected, bg), Span::styled(" ", bg)];
        spans.extend(highlight(&name, &search.query, base, matched));
        spans.push(Span::styled(" ".repeat(gap), bg));
        spans.push(Span::styled(format!("{context} "), bg.fg(view.muted)));
        f.render_widget(Paragraph::new(Line::from(spans)).style(bg), row);
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            format!("  {}", truncate_left(&search.hint, usize::from(hint.width).saturating_sub(2))),
            dim,
        )),
        hint,
    );
}

fn highlight(name: &str, query: &str, base: Style, matched: Style) -> Vec<Span<'static>> {
    let Some((start, end)) = crate::search::find(name, query) else {
        return vec![Span::styled(name.to_string(), base)];
    };
    let part = |from: usize, to: usize| name.chars().skip(from).take(to - from).collect::<String>();
    vec![
        Span::styled(part(0, start), base),
        Span::styled(part(start, end), matched),
        Span::styled(part(end, name.chars().count()), base),
    ]
}

fn draw_bar(f: &mut Frame, view: &View, areas: &Areas) {
    let surface = Style::default().bg(view.surface());
    f.render_widget(Block::new().style(surface), areas.bar);
    if view.search().is_some() {
        draw_search_bar(f, view, middle(areas.bar));
        return;
    }
    let pressed = Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
    let r = areas.todo_button;
    let mut right = areas.search_button.width + r.width;
    let lit = view.todo.is_some() || sidebar_hovered(view, r);
    let style = if lit { pressed } else { surface.fg(view.muted) };
    draw_band(f, r, todo_icon(view.todo_dot && view.todo_pending, r.width, style), style);
    if view.has_project {
        let r = areas.files_button;
        right += r.width;
        let style = if view.files.is_some() || sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
        draw_band(f, r, Span::styled(centered(FILES_ICON, r.width), style), style);
    }
    if let Some(button) = &view.changes_button {
        let r = areas.changes_button;
        right += r.width;
        let style = if button.open || sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
        draw_band(f, r, Span::styled(centered(CHANGES_ICON, r.width), style), style);
    }
    let menu = Rect { width: areas.bar.width.saturating_sub(right), ..areas.bar };
    let icon = Rect { width: COMPACT_BUTTON_WIDTH.min(menu.width), ..menu };
    let lit = view.nav.is_some() || sidebar_hovered(view, menu);
    let style = if lit { pressed } else { surface.fg(Color::Cyan) };
    draw_band(f, icon, menu_label(view.muted, view.attention, icon.width, style, lit), style);
    let crumbs = Rect { x: icon.right() + 2, width: menu.width.saturating_sub(icon.width + 2), ..middle(menu) };
    f.render_widget(Paragraph::new(Line::from(breadcrumb(view, usize::from(crumbs.width)))), crumbs);
    let r = areas.search_button;
    let style = if sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
    draw_band(f, r, Span::styled(centered(SEARCH_ICON.trim(), r.width), style), style);
}

fn line_colour(light: bool) -> Color {
    if light { LIGHT_LINE } else { DARK_LINE }
}

fn surface_colour(light: bool) -> Color {
    if light { LIGHT_SURFACE } else { DARK_SURFACE }
}

fn hover_colour(light: bool) -> Color {
    if light { LIGHT_HOVER } else { DARK_HOVER }
}

pub fn muted(theme: &HostTheme) -> Color {
    if theme.muted_is_readable() {
        Color::DarkGray
    } else if theme.is_light() == Some(true) {
        LIGHT_MUTED
    } else {
        DARK_MUTED
    }
}

fn middle(r: Rect) -> Rect {
    Rect { y: r.y + r.height.saturating_sub(1) / 2, height: r.height.min(1), ..r }
}

fn centered(text: &str, width: u16) -> String {
    format!("{text:^width$}", width = usize::from(width))
}

fn draw_band<'a>(f: &mut Frame, r: Rect, line: impl Into<Line<'a>>, style: Style) {
    f.render_widget(Block::new().style(style), r);
    f.render_widget(Paragraph::new(line.into()).style(style), middle(r));
}

fn breadcrumb(view: &View, room: usize) -> Vec<Span<'static>> {
    let Some(project) = view.projects.get(view.active).filter(|_| view.has_project) else {
        return wordmark();
    };
    let workspace = view.workspaces.get(view.active_workspace);
    let tab = workspace.zip(view.active_tab).and_then(|(w, t)| w.tabs.get(t));
    let crumbs: Vec<&str> =
        [Some(project.name.as_str()), workspace.map(|w| w.name.as_str()), tab.map(|t| t.name.as_str())]
            .into_iter()
            .flatten()
            .collect();
    let text = truncate_right(&crumbs.join(CRUMB_SEPARATOR), room);
    let project_len = project.name.chars().count().min(text.chars().count());
    let bold = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
    let (head, rest): (String, String) =
        (text.chars().take(project_len).collect(), text.chars().skip(project_len).collect());
    vec![Span::styled(head, bold), Span::styled(rest, Style::default().fg(Color::Gray))]
}

fn draw_back(f: &mut Frame, view: &View, areas: &Areas, name: &str) {
    let style = button_style(view, areas.back, Style::default().fg(Color::Cyan), Color::Cyan);
    draw_button(f, areas.back, "", BACK_LABEL, style);
    let rest =
        Rect { x: areas.back.right(), width: areas.bar.right().saturating_sub(areas.back.right()), ..areas.back };
    let max = usize::from(rest.width).saturating_sub(1);
    let style = Style::default().fg(view.muted).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(format!(" {}", truncate_right(name, max)), style)), middle(rest));
}

fn draw_title(f: &mut Frame, muted: Color, r: Rect, title: &str) {
    let style = Style::default().fg(muted).add_modifier(Modifier::BOLD);
    f.render_widget(Paragraph::new(Span::styled(format!(" {title}"), style)), middle(r));
}

fn draw_separator(f: &mut Frame, colour: Color, r: Rect) {
    let line = "─".repeat(usize::from(r.width.saturating_sub(2)));
    f.render_widget(Paragraph::new(Span::styled(format!(" {line}"), Style::default().fg(colour))), r);
}

fn button_style(view: &View, r: Rect, idle: Style, hover_bg: Color) -> Style {
    if sidebar_hovered(view, r) {
        Style::default().fg(Color::Black).bg(hover_bg).add_modifier(Modifier::BOLD)
    } else {
        idle
    }
}

fn draw_button(f: &mut Frame, r: Rect, indent: &str, label: &str, style: Style) {
    if r.height > 1
        && let Some(bg) = style.bg
    {
        f.render_widget(Block::new().style(Style::default().bg(bg)), r);
    }
    let line = Line::from(vec![Span::raw(indent), Span::styled(format!(" {label} "), style)]);
    f.render_widget(Paragraph::new(line), middle(r));
}

fn draw_workspaces(f: &mut Frame, view: &View, areas: &Areas) {
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
enum Guide {
    Top,
    Tee,
    End,
    Bar,
}

fn draw_guide(f: &mut Frame, colour: Color, x: u16, r: Rect, guide: Guide) {
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

struct Band {
    r: Rect,
    pitch: u16,
    lead: Vec<Span<'static>>,
    bg: Style,
}

impl Band {
    fn lead_width(&self) -> usize {
        self.lead.iter().map(Span::width).sum()
    }

    fn room(&self) -> usize {
        let buttons = row_close_button(self.r, self.pitch).width + row_menu_button(self.r, self.pitch).width;
        usize::from(self.r.width).saturating_sub(self.lead_width() + usize::from(buttons) + 1)
    }
}

fn draw_workspace_band(f: &mut Frame, view: &View, band: Band, entry: &WorkspaceEntry, active: bool, badge: bool) {
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

fn draw_tab_band(f: &mut Frame, view: &View, band: Band, tab: &TabEntry, active: bool) {
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

fn draw_issues_row(f: &mut Frame, view: &View, areas: &Areas) {
    let changes = view.changes_button.as_ref().filter(|_| !areas.compact());
    if view.issues {
        draw_separator(f, view.line(), areas.workspaces_separator);
        let issues = match changes {
            Some(button) => {
                let start = changes_button(areas.issues, &button.label).x;
                Rect { width: start.saturating_sub(areas.issues.x), ..areas.issues }
            }
            None => areas.issues,
        };
        let style = button_style(view, issues, Style::default().fg(view.muted), Color::Cyan);
        draw_button(f, issues, " ", ISSUES_LABEL, style);
    }
    if let Some(button) = changes {
        draw_panel_button(f, view, changes_button(areas.issues, &button.label), &button.label, button.open);
    }
    if !areas.compact() {
        draw_todo_button(f, view, areas);
        draw_panel_button(f, view, areas.files_button, files::LABEL, view.files.is_some());
    }
}

fn draw_panel_button(f: &mut Frame, view: &View, r: Rect, label: &str, open: bool) {
    let idle = if open {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(view.muted)
    };
    draw_button(f, r, "", label, button_style(view, r, idle, Color::Cyan));
}

fn draw_todo_button(f: &mut Frame, view: &View, areas: &Areas) {
    draw_panel_button(f, view, areas.todo_button, TODO_LABEL, view.todo.is_some());
    if let Some(r) = areas.todo_dot().filter(|_| view.todo_dot && view.todo_pending) {
        f.render_widget(Span::styled(TODO_DOT, Style::default().fg(view.muted)), r);
    }
}

fn todo_icon(pending: bool, width: u16, style: Style) -> Line<'static> {
    if !pending {
        return Line::from(Span::styled(centered(TODO_ICON, width), style));
    }
    let width = usize::from(width);
    let left = width.saturating_sub(1) / 2;
    Line::from(Span::styled(
        format!("{}{TODO_ICON} {TODO_DOT}{}", " ".repeat(left), " ".repeat(width.saturating_sub(left + 3))),
        style,
    ))
}

fn draw_details(
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

fn memory_size(bytes: u64) -> String {
    let megabytes = bytes.saturating_add(MB / 2) / MB;
    if megabytes < 1024 {
        return format!("{megabytes} MB");
    }
    let tenths = bytes.saturating_mul(10).saturating_add(GB / 2) / GB;
    format!("{}.{} GB", tenths / 10, tenths % 10)
}

fn status_icon(muted: Color, status: Status) -> Span<'static> {
    let (glyph, style) = match status {
        Status::Idle => ("○", Style::default().fg(muted)),
        Status::Working => ("◐", Style::default().fg(Color::Yellow)),
        Status::Done => ("✓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Status::Waiting => ("!", Style::default().fg(WAITING_COLOR).add_modifier(Modifier::BOLD)),
    };
    Span::styled(glyph, style)
}

fn group_attention(view: &View, g: usize) -> Option<Status> {
    activity::attention(view.projects.iter().filter(|p| p.group == Some(g)).map(|p| p.status))
}

fn menu_label(muted: Color, attention: Option<Status>, width: u16, style: Style, lit: bool) -> Line<'static> {
    let Some(status) = attention else { return Line::from(Span::styled(centered(MENU_ICON, width), style)) };
    let width = usize::from(width);
    let left = width.saturating_sub(1) / 2;
    let badge = status_icon(muted, status);
    let badge = if lit { Span::styled(badge.content, style) } else { badge };
    Line::from(vec![
        Span::styled(format!("{}{MENU_ICON} ", " ".repeat(left)), style),
        badge,
        Span::styled(" ".repeat(width.saturating_sub(left + 3)), style),
    ])
}

struct Tags {
    spans: Vec<Span<'static>>,
    width: usize,
}

impl Tags {
    fn fit(tags: Vec<Span<'static>>, room: usize) -> Self {
        let mut fitted = Self { spans: Vec::new(), width: 0 };
        for tag in tags {
            let width = if fitted.is_empty() { tag.width() } else { fitted.width + 1 + tag.width() };
            if width + 1 >= room {
                break;
            }
            if !fitted.is_empty() {
                fitted.spans.push(Span::raw(" "));
            }
            fitted.spans.push(tag);
            fitted.width = width;
        }
        fitted
    }

    fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn reserved(&self) -> usize {
        if self.is_empty() { 0 } else { self.width + 1 }
    }

    fn push_onto(self, line: &mut Vec<Span<'static>>, used: usize, room: usize) {
        if self.is_empty() {
            return;
        }
        line.push(Span::raw(" ".repeat(room.saturating_sub(used + self.width))));
        line.extend(self.spans);
    }
}

fn draw_more(f: &mut Frame, muted: Color, [top, bottom]: [Rect; 2], above: Option<usize>, below: Option<usize>) {
    let dim = Style::default().fg(muted);
    for (hidden, arrow, r) in [(above, "↑", top), (below, "↓", bottom)] {
        let label = match hidden {
            None => continue,
            Some(0) => format!("  {arrow} more"),
            Some(n) => format!("  {arrow} {n} more"),
        };
        f.render_widget(Paragraph::new(Span::styled(label, dim)), r);
    }
}

fn draw_row_buttons(f: &mut Frame, view: &View, row: Rect, pitch: u16, bg: Style) {
    let hover = sidebar_hovered(view, row);
    if !hover && pitch == 1 {
        return;
    }
    let style = |r: Rect, lit: Color| {
        if hover && hovered(view, r) { bg.fg(lit).add_modifier(Modifier::BOLD) } else { bg.fg(view.muted) }
    };
    let menu = row_menu_button(row, pitch);
    let menu_style = style(menu, Color::Cyan);
    draw_band(
        f,
        menu,
        Span::styled(format!("{ROW_MENU_ICON:>width$}", width = usize::from(menu.width)), menu_style),
        menu_style,
    );
    let close = row_close_button(row, pitch);
    let close_style = style(close, Color::Red);
    draw_band(f, close, Span::styled(centered("×", close.width), close_style), close_style);
}

fn draw_settings_button(f: &mut Frame, view: &View, r: Rect) {
    let style = button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
    draw_button(f, r, " ", "Settings", style);
}

fn draw_usage_button(f: &mut Frame, view: &View, r: Rect) {
    let style = button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
    draw_button(f, r, " ", USAGE_LABEL, style);
}

fn draw_quit_button(f: &mut Frame, view: &View, r: Rect) {
    let style = button_style(view, r, Style::default().fg(view.muted), Color::Red);
    draw_button(f, r, " ", "Quit", style);
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

fn marker(shown: bool) -> Span<'static> {
    Span::styled(if shown { "▌ " } else { "  " }, Style::default().fg(Color::Cyan))
}

fn draw_rail(f: &mut Frame, r: Rect) {
    let buf = f.buffer_mut();
    for y in r.top()..r.bottom() {
        let cell = &mut buf[(r.x, y)];
        if cell.symbol() == " " {
            cell.set_symbol("▌").set_fg(Color::Cyan);
        }
    }
}

fn arrow(muted: Color, folded: bool) -> Span<'static> {
    Span::styled(if folded { "▸ " } else { "▾ " }, Style::default().fg(muted))
}

fn draw_group(f: &mut Frame, view: &View, g: usize, band: Band) {
    let group = &view.groups[g];
    let count = if group.collapsed && view.counts {
        format!(" ({})", view.projects.iter().filter(|p| p.group == Some(g)).count())
    } else {
        String::new()
    };
    let badge =
        group.collapsed.then(|| group_attention(view, g)).flatten().map(|status| status_icon(view.muted, status));
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

fn draw_project_band(f: &mut Frame, view: &View, band: Band, p: usize, summary: bool) {
    let entry = &view.projects[p];
    let title_style = if p == view.active {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let count = if summary && view.counts { format!(" ({})", entry.workspaces) } else { String::new() };
    let room = band.room();
    let badge = entry.status.filter(|_| summary).map(|status| status_icon(view.muted, status));
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

fn draw_agents(f: &mut Frame, view: &View, agents: &AgentsView, areas: &Areas) {
    if areas.agents.is_empty() {
        if !areas.agents_button.is_empty() {
            let style = button_style(view, areas.agents_button, Style::default().fg(Color::Cyan), Color::Cyan);
            draw_button(f, areas.agents_button, "", AGENTS_LABEL, style);
        }
        return;
    }
    draw_separator(f, view.line(), areas.agents_border);
    draw_border(f, view, areas.agents_border, Border::Agents);
    if areas.compact() {
        draw_back(f, view, areas, AGENTS_TITLE);
    } else {
        draw_title(f, view.muted, areas.agents_title, AGENTS_TITLE);
    }
    let list = areas.agents_list;
    let rows = agent_rows(list, areas.pitch, agents.entries.len(), agents.scroll);
    if agents.entries.is_empty() {
        let first = Rect { height: areas.pitch.min(list.height), ..list };
        f.render_widget(Paragraph::new(Span::styled(format!("  {NO_AGENTS}"), dim(view.muted))), middle(first));
    }
    for (i, entry) in agents.entries.iter().enumerate() {
        let r = rows.item(i);
        if !r.is_empty() {
            draw_agent(f, view, r, areas.pitch, entry);
        }
    }
    let indices: Vec<usize> = (0..agents.entries.len()).collect();
    draw_hidden(f, view.muted, &rows, &indices, |_| true);
}

fn draw_agent(f: &mut Frame, view: &View, r: Rect, pitch: u16, entry: &AgentEntry) {
    let bg = view.row_background(r, entry.active);
    let mut line = vec![marker(entry.active)];
    if let Some(status) = entry.status {
        line.extend([status_icon(view.muted, status), Span::raw(" ")]);
    }
    let indent = line.iter().map(Span::width).sum::<usize>();
    let room = usize::from(r.width).saturating_sub(indent + 1);
    let colour = if entry.active { Color::White } else { Color::Gray };
    line.extend(agent_place(entry, room, Style::default().fg(colour), dim(view.muted)));
    draw_band(f, r, Line::from(line), bg);
    draw_details(f, view.muted, (r, row_close_button(r, pitch)), &entry.details, Some(&entry.agent), indent);
    if entry.active {
        draw_rail(f, r);
    }
}

fn agent_place(entry: &AgentEntry, room: usize, name: Style, separator: Style) -> Vec<Span<'static>> {
    let project = entry.project.chars().count();
    let rest = room.saturating_sub(project + CRUMB_SEPARATOR.chars().count());
    match entry.workspace.as_deref().filter(|_| rest >= MIN_WORKSPACE_WIDTH) {
        Some(workspace) => vec![
            Span::styled(entry.project.clone(), name),
            Span::styled(CRUMB_SEPARATOR, separator),
            Span::styled(truncate_right(workspace, rest), name),
        ],
        None => vec![Span::styled(truncate_right(&entry.project, room), name)],
    }
}

fn tree_landing_indent(shape: &TreeShape, dragged: TreeRow, spot: Spot) -> u16 {
    match dragged {
        TreeRow::Workspace(..) | TreeRow::Tab(..) => tree_indent(shape, dragged),
        _ => spot.indent(),
    }
}

fn folded(shape: &TreeShape, row: TreeRow) -> bool {
    let project = |p: usize| shape.projects.get(p);
    match row {
        TreeRow::Project(p) => project(p).is_some_and(|p| p.collapsed),
        TreeRow::Workspace(p, w) => project(p).and_then(|p| p.workspaces.get(w)).is_some_and(|w| w.collapsed),
        _ => false,
    }
}

fn tree_guide(r: Rect, shape: &TreeShape, row: TreeRow) -> u16 {
    r.x.saturating_add(tree_indent(shape, row).saturating_sub(2))
}

fn tree_among_tabs(rows: &[TreeRow], i: usize) -> Option<TreeRow> {
    let above = i.checked_sub(1).and_then(|j| rows.get(j)).and_then(|row| row.workspace());
    let below = rows.get(i + 1).and_then(|row| match row {
        TreeRow::Tab(p, w, _) | TreeRow::NewTab(p, w) => Some((*p, *w)),
        _ => None,
    });
    above.filter(|_| above == below).map(|(p, w)| TreeRow::Tab(p, w, 0))
}

fn draw_tree(f: &mut Frame, view: &View, areas: &Areas) {
    draw_title(f, view.muted, areas.title, "Projects");
    let Some(tree) = &view.tree else { return };
    let shape = &tree.shape;
    let rows = tree_rows(shape);
    let base = tree_layout_rows(areas.list, shape, &rows, view.projects_scroll);
    let (dragged, landing) = match view.drag {
        Some(Drag::Tree(row, landing)) => (Some(row), landing),
        _ => (None, None),
    };
    let indent = dragged.zip(landing).map(|(row, l)| tree_landing_indent(shape, row, l.spot));
    let (rows, line) = landed(rows, &base, landing, TreeRow::Gap, TreeRow::Landing);
    let layout = tree_layout_rows(areas.list, shape, &rows, base.first());
    let active = (view.active, view.active_workspace, view.active_tab);
    let marked = tree_active_row(&rows, shape, active).filter(|_| view.has_project);
    let (above, below) = layout.hidden();
    let workspace = |p: usize, w: usize| tree.workspaces.get(p).and_then(|ws| ws.get(w));
    for (i, &row) in rows.iter().enumerate().take(below.start).skip(above.end) {
        let (r, mark) = (layout.item(i), marked == Some(i));
        let bg = view.row_background(r, mark || view.dragging_tree_row(row));
        let lead = " ".repeat(usize::from(tree_indent(shape, row) - 2));
        let band = |lead: Vec<Span<'static>>| Band { r, pitch: 1, lead, bg };
        match row {
            TreeRow::Gap => {}
            TreeRow::Landing => {
                draw_landing(f, r, indent);
                if let Some(tab) = tree_among_tabs(&rows, i) {
                    draw_guide(f, view.line(), tree_guide(r, shape, tab), r, Guide::Bar);
                }
            }
            TreeRow::Group(g) => draw_group(f, view, g, band(vec![marker(mark)])),
            TreeRow::Project(p) => {
                let band = band(vec![marker(mark), Span::raw(lead), arrow(view.muted, folded(shape, row))]);
                draw_project_band(f, view, band, p, folded(shape, row));
            }
            TreeRow::Workspace(p, w) => {
                let Some(entry) = workspace(p, w) else { continue };
                let fold = if shape.tab_bar { Span::raw("  ") } else { arrow(view.muted, folded(shape, row)) };
                let band = band(vec![marker(mark), Span::raw(lead), fold]);
                let active = p == view.active && w == view.active_workspace;
                draw_workspace_band(f, view, band, entry, active, shape.tab_bar || folded(shape, row));
            }
            TreeRow::Tab(p, w, tab) => {
                let Some(entry) = workspace(p, w).and_then(|w| w.tabs.get(tab)) else { continue };
                draw_tab_band(f, view, band(vec![marker(mark), Span::raw(lead)]), entry, mark);
                draw_guide(f, view.line(), tree_guide(r, shape, row), r, Guide::Tee);
            }
            TreeRow::NewTab(p, w) => {
                if !workspace(p, w).is_some_and(|entry| entry.removing) {
                    let style = button_style(view, r, Style::default().fg(view.muted), Color::Cyan);
                    draw_button(f, r, &format!("{lead} "), "+ Tab", style);
                }
                draw_guide(f, view.line(), tree_guide(r, shape, row), r, Guide::End);
            }
            TreeRow::NewWorkspace(_) => {
                let style = button_style(view, r, Style::default().fg(Color::Cyan), Color::Cyan);
                draw_button(f, r, &format!("{lead} "), "+ New workspace", style);
            }
        }
        if mark {
            draw_rail(f, r);
        }
    }
    let named = |r| matches!(r, TreeRow::Group(_) | TreeRow::Project(_) | TreeRow::Workspace(..) | TreeRow::Tab(..));
    draw_hidden(f, view.muted, &layout, &rows, named);
    draw_new_project(f, view, layout.button());
    draw_landing(f, line, indent);
}

pub fn folder_name(path: &Path, home: Option<&Path>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn truncate_right(s: &str, max: usize) -> String {
    if s.chars().count() <= max || max < 2 {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{}…", head.trim_end_matches('…'))
}

pub fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 2 {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{}", tail.trim_start_matches('…'))
}

#[cfg(test)]
mod tests;
