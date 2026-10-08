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
pub mod tab_bar;
pub mod todo;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormHit {
    Submit,
    Cancel,
    Toggle,
}

pub fn form_hit(area: Rect, submit: &str, pos: Position) -> Option<FormHit> {
    let form = form_area(area);
    let [s, c] = form_buttons(form, submit);
    if s.contains(pos) {
        Some(FormHit::Submit)
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
    let width = area.width.saturating_sub(4).min(FORM_WIDTH);
    let body = form_inner(Rect::new(0, 0, width, 3)).width;
    let lines = u16::try_from(usage_lines((Color::Reset, Color::Reset), usage, body).len()).unwrap_or(u16::MAX);
    let height = lines.saturating_add(3).min(area.height);
    Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height)
}

fn usage_rows(usage: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(form_inner(usage))
}

pub fn usage_done(area: Rect, usage: &Usage) -> Rect {
    update_button(usage_rows(usage_area(area, usage))[1], crate::settings::DONE)
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
    Picker(Picker),
    Issues(Issues),
    Settings(Settings),
    Search(Search),
    Keys(keys::Keys),
}

impl Overlay {
    fn is_modal(&self) -> bool {
        !matches!(self, Self::Menu { .. } | Self::Search(_) | Self::Keys(_))
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
    pub details: Details,
    pub others: usize,
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
        Self { name, status: None, details: Details::default(), others: 0 }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toast<'a> {
    pub message: &'a str,
    pub icon: ToastIcon,
    pub undo: bool,
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
    pub changes: Option<changes::View>,
    pub changes_button: Option<ChangesButton>,
    pub todo: Option<todo::View>,
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
    let undo = if toast.undo { UNDO_LABEL.chars().count() + 3 } else { 0 };
    let text = TOAST_ICON.chars().count() + toast.message.chars().count() + 1 + undo;
    let width = u16::try_from(text).unwrap_or(u16::MAX).saturating_add(2).min(area.width);
    let height = 3.min(area.height);
    let x = area.right().saturating_sub(width + TOAST_MARGIN).max(area.x);
    let y = area.bottom().saturating_sub(height + TOAST_MARGIN).max(area.y);
    Rect::new(x, y, width, height)
}

pub fn toast_undo(area: Rect, toast: Toast) -> Rect {
    if !toast.undo {
        return Rect::default();
    }
    let r = toast_area(area, toast).inner(Margin::new(1, 1));
    let width = button_width(UNDO_LABEL).min(r.width);
    Rect { x: r.right().saturating_sub(width + 1), width, ..r }
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
    };
    let border = Style::default().fg(icon.style.fg.unwrap_or(Color::Green));
    f.render_widget(Clear, r);
    f.render_widget(Block::bordered().border_type(BorderType::Rounded).border_style(border), r);
    f.render_widget(Paragraph::new(Line::from(vec![icon, Span::raw(toast.message)])), r.inner(Margin::new(1, 1)));
    let undo = toast_undo(f.area(), toast);
    if !undo.is_empty() {
        let lit = view.hover.is_some_and(|p| undo.contains(p));
        let style = if lit {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        };
        f.render_widget(Paragraph::new(Span::styled(format!(" {UNDO_LABEL} "), style)), undo);
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
    let [body, _] = usage_rows(r);
    f.render_widget(Paragraph::new(usage_lines((view.muted, view.line()), usage, body.width)), body);
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

fn draw_dialog_buttons(f: &mut Frame, view: &View, [submit, cancel]: [Rect; 2], label: &str, cancel_label: &str) {
    draw_submit(f, view, submit, label);
    let style = dialog_button_style(view, cancel, false);
    f.render_widget(Paragraph::new(Span::styled(format!(" {cancel_label} "), style)), cancel);
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
    draw_usage_button(f, view, areas.usage);
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
    let style = if view.todo.is_some() || sidebar_hovered(view, r) { pressed } else { surface.fg(view.muted) };
    draw_band(f, r, Span::styled(centered(TODO_ICON, r.width), style), style);
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
    let icon = tab.status.map(|status| status_icon(view.muted, status));
    let icon_width = if icon.is_some() { 2 } else { 0 };
    let indent = band.lead_width() + icon_width;
    let max = band.room().saturating_sub(icon_width);
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
    if let Some(icon) = icon {
        line.extend([icon, Span::raw(" ")]);
    }
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
        draw_panel_button(f, view, areas.todo_button, TODO_LABEL, view.todo.is_some());
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
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use rstest::rstest;

    use super::*;
    use crate::emulator::Emulator;
    use crate::host_theme::HostTheme;

    const W: u16 = 100;
    const H: u16 = 18;
    const AREA: Rect = Rect { x: 0, y: 0, width: W, height: H };
    const SMALL: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

    fn view(names: &[&str]) -> View<'static> {
        View {
            groups: Vec::new(),
            projects: names
                .iter()
                .map(|n| ProjectEntry { name: (*n).to_string(), workspaces: 1, group: None, status: None })
                .collect(),
            active: 0,
            has_project: false,
            workspaces: Vec::new(),
            active_workspace: 0,
            active_tab: None,
            issues: false,
            hover: None,
            widths: Widths::default(),
            sidebar: Sidebar::SideBySide,
            resizing: None,
            light: false,
            muted: Color::DarkGray,
            projects_scroll: 0,
            workspaces_scroll: 0,
            tab: None,
            overlay: None,
            toast: None,
            nav: None,
            update: None,
            changes: None,
            changes_button: None,
            todo: None,
            files: None,
            attention: None,
            drag: None,
            tree: None,
            agents: None,
            counts: true,
            tab_bar: None,
        }
    }

    fn plain(projects: usize) -> Vec<SidebarRow> {
        sidebar_rows(&vec![None; projects], &[])
    }

    fn tabs(counts: &[usize]) -> TabLines {
        TabLines::from(counts.iter().map(|&n| vec![Details::default().lines(); n]).collect::<Vec<_>>())
    }

    fn render(view: &View) -> Terminal<TestBackend> {
        render_sized(view, W, H)
    }

    fn render_sized(view: &View, width: u16, height: u16) -> Terminal<TestBackend> {
        let mut t = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        t.draw(|f| draw(f, view)).expect("draw");
        t
    }

    fn areas() -> Areas {
        layout(AREA, Widths::default())
    }

    fn row_text(t: &Terminal<TestBackend>, r: Rect) -> String {
        (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
    }

    fn list() -> Rect {
        areas().list
    }

    fn screen(bytes: &[u8]) -> Snapshot {
        let mut emu = Emulator::new(
            H,
            W - SIDEBAR_WIDTH - WORKSPACES_WIDTH - PANE_PADDING,
            0,
            &HostTheme::default(),
            Box::new(|_| {}),
        )
        .expect("emulator");
        emu.feed(bytes);
        emu.snapshot().expect("snapshot")
    }

    fn single(screen: Snapshot) -> TabView {
        TabView {
            layout: Node::Leaf(0),
            screens: vec![screen],
            active: 0,
            dim_inactive: true,
            dragging: None,
            link: None,
            landing: None,
        }
    }

    fn close_x() -> u16 {
        list().right() - 2
    }

    mod sidebar_setting {
        use super::*;

        #[rstest]
        #[case::side_by_side("side_by_side", Sidebar::SideBySide)]
        #[case::projects_on_top("projects_on_top", Sidebar::ProjectsOnTop)]
        #[case::workspaces_on_top("workspaces_on_top", Sidebar::WorkspacesOnTop)]
        #[case::tree("tree", Sidebar::Tree)]
        #[case::any_case_and_spaces(" Projects_On_Top ", Sidebar::ProjectsOnTop)]
        #[case::unknown("sideways", Sidebar::ProjectsOnTop)]
        #[case::empty("", Sidebar::ProjectsOnTop)]
        fn is_read_from_the_config(#[case] setting: &str, #[case] expected: Sidebar) {
            assert_eq!(Sidebar::from_setting(setting), expected);
        }

        #[test]
        fn only_side_by_side_keeps_two_columns() {
            let stacked: Vec<bool> = Sidebar::ALL.into_iter().map(Sidebar::stacked).collect();
            assert_eq!(stacked, [false, true, true, true]);
        }

        #[test]
        fn each_choice_is_its_id_with_a_note() {
            let ids: Vec<&str> = Sidebar::choices().into_iter().map(|(id, _)| id).collect();
            assert_eq!(ids, ["side_by_side", "projects_on_top", "workspaces_on_top", "tree"]);
        }
    }

    mod changes_layout {
        use super::*;

        const BIG: Rect = Rect { x: 0, y: 0, width: 160, height: 30 };

        #[test]
        fn the_panel_sits_right_of_the_pane() {
            let a = layout_with(BIG, Widths::default(), true, Sidebar::SideBySide);
            assert_eq!(
                (a.changes_border.x, a.changes.x, a.changes.right(), a.pane.right()),
                (a.pane.right(), a.pane.right() + 1, BIG.right(), BIG.width - Widths::default().changes_width(160))
            );
        }

        #[test]
        fn the_panel_border_drags() {
            let a = layout_with(BIG, Widths::default(), true, Sidebar::SideBySide);
            assert_eq!(a.border_hit(a.changes_border.as_position()), Some(Border::Changes));
        }

        #[test]
        fn a_compact_panel_covers_the_screen_below_the_bar() {
            let small = Rect { width: 80, ..BIG };
            let a = layout_with(small, Widths::default(), true, Sidebar::SideBySide);
            assert_eq!(
                (a.changes, a.changes_button.right(), a.files_button.right(), a.todo_button.right()),
                (a.pane, a.files_button.x, a.todo_button.x, a.search_button.x)
            );
        }

        fn with_panel() -> View<'static> {
            let tints = crate::changes::Tints::of(&HostTheme::default());
            let panel = changes::View {
                mode: crate::changes::Mode::Uncommitted,
                base: None,
                body: changes::Body::Loading,
                folded: Vec::new(),
                viewed: Vec::new(),
                gaps: HashMap::new(),
                scroll: 0,
                live: false,
                light: false,
                muted: Color::DarkGray,
                tints,
                filter: None,
            };
            View {
                has_project: true,
                issues: true,
                workspaces: vec![WorkspaceEntry {
                    name: "main".into(),
                    tabs: vec!["zsh".into()],
                    behind: 0,
                    removing: false,
                }],
                active_tab: Some(0),
                changes: Some(panel),
                changes_button: Some(ChangesButton { label: "Changes 3".into(), open: true }),
                ..view(&["shop"])
            }
        }

        #[test]
        fn draws_the_panel_and_its_button() {
            insta::assert_snapshot!(render_sized(&with_panel(), 140, 16).backend());
        }

        #[test]
        fn hovering_the_changes_button_leaves_issues_unlit() {
            let r = changes_button(layout(Rect::new(0, 0, 140, 16), Widths::default()).issues, "Changes 3");
            let t = render_sized(&View { hover: Some(r.as_position()), ..with_panel() }, 140, 16);
            let issues_x = layout(Rect::new(0, 0, 140, 16), Widths::default()).issues.x + 2;
            let buffer = t.backend().buffer();
            assert_eq!((buffer[(issues_x, r.y)].bg, buffer[(r.x + 1, r.y)].bg), (Color::Reset, Color::Cyan));
        }

        fn with_todo() -> View<'static> {
            let item = |id: u64, text: &str, done: bool| todo::Item { id, text: text.into(), done, editing: None };
            let panel = todo::View {
                items: vec![
                    item(1, "fix the login bug in safari", false),
                    item(2, "renew the domain", false),
                    item(3, "write docs", true),
                ],
                scroll: 0,
                adding: None,
                light: false,
                muted: Color::DarkGray,
                drag: None,
            };
            View { changes: None, todo: Some(panel), ..with_panel() }
        }

        #[test]
        fn draws_the_todo_panel_and_its_button() {
            insta::assert_snapshot!(render_sized(&with_todo(), 140, 18).backend());
        }

        #[test]
        fn a_compact_todo_panel_covers_the_screen_below_the_bar() {
            insta::assert_snapshot!(render_sized(&with_todo(), 80, 18).backend());
        }

        #[test]
        fn the_bar_has_a_changes_button_in_compact_mode() {
            let v = View {
                changes: None,
                changes_button: Some(ChangesButton { label: "Changes".into(), open: false }),
                ..with_panel()
            };
            let t = render_sized(&v, 80, 16);
            let r = layout(Rect::new(0, 0, 80, 16), Widths::default()).changes_button;
            assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "±");
        }
    }

    mod layout {
        use super::*;

        #[test]
        fn sidebar_has_fixed_width_on_the_left_below_the_header() {
            assert_eq!(areas().sidebar, Rect::new(0, HEADER_HEIGHT, SIDEBAR_WIDTH, H - HEADER_HEIGHT));
        }

        #[test]
        fn workspaces_column_sits_right_of_the_sidebar() {
            assert_eq!(areas().workspaces, Rect::new(SIDEBAR_WIDTH, 0, WORKSPACES_WIDTH, H));
        }

        #[test]
        fn workspaces_list_starts_level_with_the_projects_list() {
            assert_eq!((areas().workspaces_list.y, areas().workspaces_title.y), (list().y, areas().title.y));
        }

        #[test]
        fn pane_takes_the_rest_after_a_padding_column() {
            let x = SIDEBAR_WIDTH + WORKSPACES_WIDTH + PANE_PADDING;
            assert_eq!(areas().pane, Rect::new(x, 0, W - x, H));
        }

        #[test]
        fn search_spans_both_columns_on_the_first_row() {
            let search = areas().search;
            assert_eq!(search, Rect::new(1, 0, SIDEBAR_WIDTH + WORKSPACES_WIDTH - 3, 1));
        }

        #[test]
        fn title_leaves_a_blank_row_below_the_search() {
            assert_eq!(areas().title.y, areas().search.bottom() + 1);
        }

        #[test]
        fn list_leaves_a_blank_row_below_the_title() {
            assert_eq!(list().y, areas().title.bottom() + GAP);
        }

        #[test]
        fn list_ends_before_the_border() {
            assert_eq!(list().right(), SIDEBAR_WIDTH - 1);
        }

        #[test]
        fn list_ends_right_above_the_separator() {
            assert_eq!(list().bottom(), areas().separator.y);
        }

        #[test]
        fn settings_and_usage_buttons_sit_right_above_the_quit_button() {
            let row = |y| Rect::new(0, y, SIDEBAR_WIDTH - 1, 1);
            assert_eq!((areas().settings, areas().usage), (row(H - 3), row(H - 2)));
        }

        #[test]
        fn quit_button_takes_the_last_row() {
            assert_eq!(areas().quit, Rect::new(0, H - 1, SIDEBAR_WIDTH - 1, 1));
        }

        #[test]
        fn results_cover_both_columns_below_the_header() {
            let results = areas().results;
            assert_eq!(results, Rect::new(0, HEADER_HEIGHT, SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1, H - HEADER_HEIGHT));
        }
    }

    mod widths {
        use super::*;

        const WIDE: Widths = Widths { projects: 40, workspaces: 30, changes: None, stack: None, agents: None };

        #[test]
        fn the_changes_panel_takes_half_of_the_free_space_up_to_its_default() {
            let free = 160 - SIDEBAR_WIDTH - WORKSPACES_WIDTH - PANE_PADDING;
            let widths = Widths::default();
            assert_eq!((widths.changes_width(160), widths.changes_width(300)), (free / 2, changes::DEFAULT_WIDTH));
        }

        #[test]
        fn a_dragged_changes_panel_keeps_its_width() {
            let widths = Widths::default().dragged(Border::Changes, 100, 200);
            assert_eq!((widths.changes, widths.changes_width(200)), (Some(100), 100));
        }

        #[test]
        fn the_changes_panel_leaves_room_for_the_pane_and_columns() {
            let widths = Widths { changes: Some(90), ..Widths::default() };
            assert_eq!(widths.changes_width(100), 100 - PANE_PADDING - MIN_PANE_WIDTH - 2 * MIN_COLUMN_WIDTH);
        }

        #[test]
        fn a_double_click_makes_the_changes_panel_automatic_again() {
            let widths = Widths { changes: Some(90), ..Widths::default() };
            assert_eq!(widths.reset(Border::Changes).changes, None);
        }

        #[test]
        fn fit_keeps_widths_that_leave_room_for_the_pane() {
            assert_eq!(WIDE.fit(W), WIDE);
        }

        #[test]
        fn fit_shrinks_the_workspaces_column_first() {
            let room = 80 - PANE_PADDING - MIN_PANE_WIDTH;
            assert_eq!(WIDE.fit(80), Widths { projects: 40, workspaces: room - 40, ..Widths::default() });
        }

        #[test]
        fn fit_shrinks_the_projects_column_once_workspaces_is_at_its_minimum() {
            let room = 60 - PANE_PADDING - MIN_PANE_WIDTH;
            assert_eq!(
                WIDE.fit(60),
                Widths { projects: room - MIN_COLUMN_WIDTH, workspaces: MIN_COLUMN_WIDTH, ..Widths::default() }
            );
        }

        #[test]
        fn fit_never_goes_below_the_minimum() {
            let min = Widths { projects: MIN_COLUMN_WIDTH, workspaces: MIN_COLUMN_WIDTH, ..Widths::default() };
            assert_eq!(WIDE.fit(20), min);
        }

        #[test]
        fn dragging_the_projects_border_puts_it_under_the_mouse() {
            assert_eq!(Widths::default().dragged(Border::Projects, 39, W).projects, 40);
        }

        #[test]
        fn dragging_the_workspaces_border_puts_it_under_the_mouse() {
            let widths = Widths::default().dragged(Border::Workspaces, SIDEBAR_WIDTH + 29, W);
            assert_eq!(widths, Widths { projects: SIDEBAR_WIDTH, workspaces: 30, ..Widths::default() });
        }

        #[rstest]
        #[case::projects(Border::Projects)]
        #[case::workspaces(Border::Workspaces)]
        fn dragging_stops_at_the_minimum_width(#[case] border: Border) {
            let widths = Widths::default().dragged(border, 0, W);
            let width = if border == Border::Projects { widths.projects } else { widths.workspaces };
            assert_eq!(width, MIN_COLUMN_WIDTH);
        }

        #[rstest]
        #[case::projects(Border::Projects)]
        #[case::workspaces(Border::Workspaces)]
        fn dragging_leaves_room_for_the_pane(#[case] border: Border) {
            let widths = Widths::default().dragged(border, W - 1, W);
            assert_eq!(layout(AREA, widths).pane.width, MIN_PANE_WIDTH);
        }

        #[test]
        fn reset_brings_back_the_default_width_of_that_column_only() {
            assert_eq!(
                WIDE.reset(Border::Projects),
                Widths { projects: SIDEBAR_WIDTH, workspaces: 30, ..Widths::default() }
            );
        }

        const STACKED: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

        #[test]
        fn the_line_starts_halfway() {
            assert_eq!(Widths::default().top_rows(20), 10);
        }

        #[test]
        fn a_dragged_line_keeps_its_rows() {
            assert_eq!(Widths { stack: Some(7), ..Widths::default() }.top_rows(20), 7);
        }

        #[rstest]
        #[case::top(0, MIN_STACK_SECTION)]
        #[case::bottom(40, 20 - MIN_STACK_SECTION)]
        fn the_line_leaves_room_for_both_lists(#[case] stack: u16, #[case] expected: u16) {
            assert_eq!(Widths { stack: Some(stack), ..Widths::default() }.top_rows(20), expected);
        }

        #[test]
        fn a_saved_line_too_low_for_a_short_screen_is_fitted() {
            assert_eq!(Widths { stack: Some(30), ..Widths::default() }.top_rows(12), 12 - MIN_STACK_SECTION);
        }

        #[test]
        fn a_room_too_short_for_both_lists_is_split_in_half() {
            assert_eq!(Widths { stack: Some(1), ..Widths::default() }.top_rows(7), 3);
        }

        #[test]
        fn dragging_the_line_puts_it_under_the_mouse() {
            let widths =
                Widths::default().stacked_dragged(Border::Stack, Position::new(5, HEADER_HEIGHT + 12), STACKED, false);
            assert_eq!(widths.stack, Some(12));
        }

        #[test]
        fn the_stacked_column_can_take_the_room_of_the_workspaces_column() {
            let widths = Widths::default().stacked_dragged(Border::Projects, Position::new(59, 6), STACKED, false);
            assert_eq!(widths.projects, 60);
        }

        #[test]
        fn the_stacked_column_leaves_room_for_the_pane() {
            let widths = Widths::default().stacked_dragged(Border::Projects, Position::new(W - 1, 6), STACKED, false);
            assert_eq!(widths.stacked_width(W), W - PANE_PADDING - MIN_PANE_WIDTH);
        }

        #[test]
        fn a_double_click_splits_the_lists_in_half_again() {
            assert_eq!(Widths { stack: Some(7), ..Widths::default() }.reset(Border::Stack).stack, None);
        }

        #[test]
        fn two_columns_ignore_the_line() {
            assert_eq!(Widths::default().dragged(Border::Stack, 10, W), Widths::default());
        }
    }

    mod borders {
        use super::*;

        #[test]
        fn the_projects_border_is_the_last_column_of_the_sidebar() {
            assert_eq!(areas().projects_border, Rect::new(SIDEBAR_WIDTH - 1, HEADER_HEIGHT, 1, H - HEADER_HEIGHT));
        }

        #[test]
        fn the_workspaces_border_is_the_last_column_of_the_workspaces_column() {
            let x = SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1;
            assert_eq!(areas().workspaces_border, Rect::new(x, 0, 1, H));
        }

        #[test]
        fn they_follow_the_widths() {
            let widths = Widths { projects: 40, workspaces: 20, ..Widths::default() };
            let areas = layout(AREA, widths);
            assert_eq!((areas.projects_border.x, areas.workspaces_border.x, areas.pane.x), (39, 59, 61));
        }

        #[rstest]
        #[case::projects(Position::new(SIDEBAR_WIDTH - 1, HEADER_HEIGHT + 2), Some(Border::Projects))]
        #[case::workspaces(Position::new(SIDEBAR_WIDTH + WORKSPACES_WIDTH - 1, 0), Some(Border::Workspaces))]
        #[case::header_has_no_projects_border(Position::new(SIDEBAR_WIDTH - 1, 0), None)]
        #[case::inside_the_list(Position::new(3, HEADER_HEIGHT + 2), None)]
        fn hit_finds_the_border_under_the_mouse(#[case] pos: Position, #[case] expected: Option<Border>) {
            assert_eq!(areas().border_hit(pos), expected);
        }

        fn middle(border: Border) -> Position {
            let r = areas().border(border);
            Position::new(r.x, r.y + r.height / 2)
        }

        #[test]
        fn a_border_is_dim_by_default() {
            let pos = middle(Border::Projects);
            assert_eq!(render(&view(&["~"])).backend().buffer()[pos].fg, DARK_LINE);
        }

        #[test]
        fn a_border_turns_cyan_on_hover() {
            let pos = middle(Border::Workspaces);
            let v = View { hover: Some(pos), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[pos].fg, Color::Cyan);
        }

        #[test]
        fn a_border_stays_cyan_while_it_is_dragged() {
            let pos = middle(Border::Projects);
            let v = View { resizing: Some(Border::Projects), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[pos].fg, Color::Cyan);
        }

        #[test]
        fn a_border_ignores_the_hover_while_an_overlay_is_open() {
            let pos = middle(Border::Workspaces);
            let menu = Overlay::Menu { at: Position::new(80, 1), items: vec!["rename tab".into()] };
            let v = View { hover: Some(pos), overlay: Some(menu), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[pos].fg, DARK_LINE);
        }
    }

    mod stacked {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

        fn stacked(sidebar: Sidebar) -> Areas {
            layout_with(TALL, Widths::default(), false, sidebar)
        }

        #[test]
        fn the_pane_gets_the_width_of_the_workspaces_column() {
            let side = layout(TALL, Widths::default());
            assert_eq!(stacked(Sidebar::ProjectsOnTop).pane.width, side.pane.width + WORKSPACES_WIDTH);
        }

        #[rstest]
        #[case::projects_on_top(Sidebar::ProjectsOnTop, true)]
        #[case::workspaces_on_top(Sidebar::WorkspacesOnTop, false)]
        fn the_setting_picks_the_list_on_top(#[case] sidebar: Sidebar, #[case] projects_first: bool) {
            let a = stacked(sidebar);
            assert_eq!(a.title.y < a.workspaces_title.y, projects_first);
        }

        #[test]
        fn the_line_sits_between_the_lists() {
            let a = stacked(Sidebar::ProjectsOnTop);
            assert_eq!(
                (a.list.bottom(), a.stack_border.y, a.workspaces_title.y),
                (HEADER_HEIGHT + 10, HEADER_HEIGHT + 10, HEADER_HEIGHT + 11)
            );
        }

        #[test]
        fn one_footer_holds_issues_todo_settings_usage_and_quit() {
            let a = stacked(Sidebar::WorkspacesOnTop);
            assert_eq!(
                (a.separator.y, a.issues.y, a.todo_button.y, a.settings.y, a.usage.y, a.quit.y, a.workspaces_separator),
                (24, 25, 26, 27, 28, 29, Rect::default())
            );
        }

        #[test]
        fn the_column_border_runs_the_whole_height() {
            let a = stacked(Sidebar::ProjectsOnTop);
            assert_eq!(
                (a.projects_border, a.workspaces_border),
                (Rect::new(SIDEBAR_WIDTH - 1, 0, 1, 30), Rect::default())
            );
        }

        #[rstest]
        #[case::line(Position::new(3, HEADER_HEIGHT + 10), Some(Border::Stack))]
        #[case::column(Position::new(SIDEBAR_WIDTH - 1, 2), Some(Border::Projects))]
        #[case::list(Position::new(3, HEADER_HEIGHT + 4), None)]
        fn hit_finds_the_line_and_the_column_border(#[case] pos: Position, #[case] expected: Option<Border>) {
            assert_eq!(stacked(Sidebar::ProjectsOnTop).border_hit(pos), expected);
        }

        #[test]
        fn each_list_scrolls_in_its_own_section() {
            let a = stacked(Sidebar::ProjectsOnTop);
            let (projects, workspaces) = (a.list.as_position(), a.workspaces_list.as_position());
            assert_eq!(
                (a.sidebar.contains(projects), a.sidebar.contains(workspaces), a.workspaces.contains(workspaces)),
                (true, false, true)
            );
        }

        #[test]
        fn narrow_terminals_stay_compact() {
            let a = layout_with(Rect { width: 80, ..TALL }, Widths::default(), false, Sidebar::ProjectsOnTop);
            assert!(a.compact());
        }

        #[test]
        fn the_changes_panel_sits_right_of_the_pane() {
            let a = layout_with(Rect { width: 160, ..TALL }, Widths::default(), true, Sidebar::ProjectsOnTop);
            assert_eq!((a.pane.x, a.changes_border.x), (SIDEBAR_WIDTH + PANE_PADDING, a.pane.right()));
        }

        fn with_lists(sidebar: Sidebar) -> View<'static> {
            let mut v = View {
                has_project: true,
                issues: true,
                active_tab: Some(0),
                sidebar,
                workspaces: vec![
                    WorkspaceEntry {
                        name: "login".into(),
                        tabs: vec!["claude".into(), "nvim".into()],
                        behind: 0,
                        removing: false,
                    },
                    WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, removing: false },
                ],
                ..view(&["tmp", "api", "cornercase"])
            };
            v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
            v.projects[1].group = Some(0);
            v
        }

        #[test]
        fn renders_projects_on_top() {
            insta::assert_snapshot!(render_sized(&with_lists(Sidebar::ProjectsOnTop), W, TALL.height).backend());
        }

        #[test]
        fn renders_workspaces_on_top() {
            insta::assert_snapshot!(render_sized(&with_lists(Sidebar::WorkspacesOnTop), W, TALL.height).backend());
        }

        #[test]
        fn the_column_border_runs_down_the_header_too() {
            let t = render_sized(&with_lists(Sidebar::ProjectsOnTop), W, TALL.height);
            assert_eq!(t.backend().buffer()[(SIDEBAR_WIDTH - 1, 1)].symbol(), "│");
        }

        #[test]
        fn the_hovered_line_turns_cyan() {
            let line = stacked(Sidebar::ProjectsOnTop).stack_border;
            let at = Position::new(line.x + 3, line.y);
            let t = render_sized(&View { hover: Some(at), ..with_lists(Sidebar::ProjectsOnTop) }, W, TALL.height);
            assert_eq!(t.backend().buffer()[(at.x, at.y)].fg, Color::Cyan);
        }

        #[rstest]
        #[case::projects_on_top_10_rows(Sidebar::ProjectsOnTop, 10)]
        #[case::projects_on_top_11_rows(Sidebar::ProjectsOnTop, 11)]
        #[case::workspaces_on_top_10_rows(Sidebar::WorkspacesOnTop, 10)]
        #[case::workspaces_on_top_11_rows(Sidebar::WorkspacesOnTop, 11)]
        fn a_short_terminal_still_shows_the_footer(#[case] sidebar: Sidebar, #[case] height: u16) {
            let a = layout_with(Rect { height, ..TALL }, Widths::default(), false, sidebar);
            let t = render_sized(&with_lists(sidebar), W, height);
            let text = |r: Rect| row_text(&t, r).trim().to_string();
            assert_eq!(
                [text(a.issues), text(a.settings), text(a.usage), text(a.quit)],
                ["Issues", "Settings", "Usage", "Quit"]
            );
        }

        #[test]
        fn short_terminals_do_not_panic() {
            for height in 1..=TALL.height {
                render_sized(&with_lists(Sidebar::WorkspacesOnTop), W, height);
            }
        }
    }

    mod tree_layout {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

        fn tree() -> Areas {
            layout_with(TALL, Widths::default(), false, Sidebar::Tree)
        }

        #[test]
        fn one_list_runs_from_its_title_to_the_footer() {
            let a = tree();
            assert_eq!(
                (a.title.y, a.list.y, a.list.bottom(), a.separator.y, a.issues.y, a.todo_button.y, a.quit.y),
                (HEADER_HEIGHT, HEADER_HEIGHT + 2, 24, 24, 25, 26, 29)
            );
        }

        #[test]
        fn has_no_workspaces_list_and_no_line() {
            let a = tree();
            assert_eq!([a.workspaces, a.workspaces_list, a.stack_border], [Rect::default(); 3]);
        }

        #[test]
        fn the_pane_gets_the_width_of_the_workspaces_column() {
            let side = layout(TALL, Widths::default());
            assert_eq!(tree().pane.width, side.pane.width + WORKSPACES_WIDTH);
        }

        #[test]
        fn the_wheel_scrolls_it_anywhere_in_the_column() {
            let a = tree();
            assert_eq!(
                (a.sidebar.x, a.sidebar.width, a.sidebar.contains(a.list.as_position())),
                (0, SIDEBAR_WIDTH, true)
            );
        }

        #[test]
        fn only_the_tree_says_it_is_one() {
            let trees: Vec<bool> =
                Sidebar::ALL.into_iter().map(|s| layout_with(TALL, Widths::default(), false, s).tree).collect();
            assert_eq!(trees, [false, false, false, true]);
        }

        #[test]
        fn narrow_terminals_get_the_compact_menu() {
            let a = layout_with(Rect { width: 80, ..TALL }, Widths::default(), false, Sidebar::Tree);
            assert_eq!((a.compact(), a.tree), (true, false));
        }
    }

    mod tree_rows {
        use super::*;

        const LIST: Rect = Rect { x: 0, y: 4, width: SIDEBAR_WIDTH - 1, height: 20 };

        fn shape() -> TreeShape {
            let main = WorkspaceShape { collapsed: false, tabs: vec![1, 2] };
            let login = WorkspaceShape { collapsed: true, tabs: vec![1] };
            TreeShape {
                groups: vec![false],
                tab_bar: false,
                projects: vec![
                    ProjectShape { group: None, collapsed: true, workspaces: vec![WorkspaceShape::default()] },
                    ProjectShape { group: Some(0), collapsed: false, workspaces: vec![main, login] },
                ],
            }
        }

        #[test]
        fn list_groups_projects_workspaces_and_tabs_in_order() {
            assert_eq!(
                tree_rows(&shape()),
                [
                    TreeRow::Project(0),
                    TreeRow::Gap,
                    TreeRow::Group(0),
                    TreeRow::Project(1),
                    TreeRow::Workspace(1, 0),
                    TreeRow::Tab(1, 0, 0),
                    TreeRow::Tab(1, 0, 1),
                    TreeRow::NewTab(1, 0),
                    TreeRow::Workspace(1, 1),
                    TreeRow::NewWorkspace(1),
                ]
            );
        }

        #[test]
        fn a_folded_group_hides_its_projects() {
            let mut s = shape();
            s.groups[0] = true;
            assert_eq!(tree_rows(&s), [TreeRow::Project(0), TreeRow::Gap, TreeRow::Group(0)]);
        }

        #[test]
        fn a_tab_with_its_details_takes_two_rows() {
            let rows = [TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 0, 1)].map(|r| tree_row(LIST, &shape(), 0, r).height);
            assert_eq!(rows, [1, 2]);
        }

        #[rstest]
        #[case::the_tab(shape(), (1, 0, Some(1)), TreeRow::Tab(1, 0, 1))]
        #[case::a_folded_workspace(shape(), (1, 1, Some(0)), TreeRow::Workspace(1, 1))]
        #[case::a_workspace_without_tabs(shape(), (1, 0, None), TreeRow::Workspace(1, 0))]
        #[case::a_folded_project(shape(), (0, 0, Some(0)), TreeRow::Project(0))]
        #[case::a_folded_group(TreeShape { groups: vec![true], ..shape() }, (1, 0, Some(0)), TreeRow::Group(0))]
        fn the_active_mark_goes_on_the_nearest_row_shown(
            #[case] shape: TreeShape,
            #[case] active: (usize, usize, Option<usize>),
            #[case] expected: TreeRow,
        ) {
            let rows = tree_rows(&shape);
            assert_eq!(tree_active_row(&rows, &shape, active).map(|i| rows[i]), Some(expected));
        }

        #[test]
        fn rows_are_indented_one_step_per_level() {
            let arrows = [TreeRow::Group(0), TreeRow::Project(0), TreeRow::Project(1), TreeRow::Workspace(1, 0)]
                .map(|r| tree_arrow(LIST, &shape(), 0, r).x);
            assert_eq!(arrows, [2, 2, 4, 6]);
        }

        fn hit(row: TreeRow, x: impl Fn(Rect) -> u16) -> Option<TreeHit> {
            let r = tree_row(LIST, &shape(), 0, row);
            tree_hit(LIST, &shape(), 0, Position::new(x(r), r.y))
        }

        #[rstest]
        #[case::the_arrow_folds(TreeRow::Workspace(1, 0), 6, Some(TreeHit::Fold(TreeRow::Workspace(1, 0))))]
        #[case::the_name_selects(TreeRow::Workspace(1, 0), 9, Some(TreeHit::Select(TreeRow::Workspace(1, 0))))]
        #[case::a_project_name_selects(TreeRow::Project(1), 7, Some(TreeHit::Select(TreeRow::Project(1))))]
        #[case::a_group_folds_anywhere(TreeRow::Group(0), 10, Some(TreeHit::Fold(TreeRow::Group(0))))]
        #[case::a_tab_selects(TreeRow::Tab(1, 0, 0), 2, Some(TreeHit::Select(TreeRow::Tab(1, 0, 0))))]
        #[case::add_a_tab(TreeRow::NewTab(1, 0), 9, Some(TreeHit::NewTab(1, 0)))]
        #[case::add_a_workspace(TreeRow::NewWorkspace(1), 7, Some(TreeHit::NewWorkspace(1)))]
        #[case::a_gap_is_nothing(TreeRow::Gap, 4, None)]
        fn a_click_lands_on_what_is_under_it(#[case] row: TreeRow, #[case] x: u16, #[case] expected: Option<TreeHit>) {
            assert_eq!(hit(row, |r| r.x + x), expected);
        }

        #[rstest]
        #[case::a_group(TreeRow::Group(0))]
        #[case::a_project(TreeRow::Project(1))]
        #[case::a_workspace(TreeRow::Workspace(1, 0))]
        #[case::a_tab(TreeRow::Tab(1, 0, 1))]
        fn the_close_button_closes_the_row(#[case] row: TreeRow) {
            let x = tree_close(LIST, &shape(), 0, row);
            assert_eq!(tree_hit(LIST, &shape(), 0, x.as_position()), Some(TreeHit::Close(row)));
        }

        #[rstest]
        #[case::a_group(TreeRow::Group(0))]
        #[case::a_project(TreeRow::Project(1))]
        #[case::a_workspace(TreeRow::Workspace(1, 0))]
        #[case::a_tab(TreeRow::Tab(1, 0, 1))]
        fn the_menu_button_opens_the_row_menu(#[case] row: TreeRow) {
            let menu = row_menu_button(tree_row(LIST, &shape(), 0, row), 1);
            assert_eq!(tree_hit(LIST, &shape(), 0, menu.as_position()), Some(TreeHit::Menu(row)));
        }

        #[test]
        fn the_button_at_the_bottom_adds_a_project() {
            let rows = tree_rows(&shape());
            let button = tree_layout_rows(LIST, &shape(), &rows, 0).button();
            assert_eq!(tree_hit(LIST, &shape(), 0, button.as_position()), Some(TreeHit::NewProject));
        }
    }

    mod muted {
        use libghostty_vt::style::RgbColor;
        use rstest::rstest;

        use super::*;
        use crate::host_theme::HostTheme;

        #[rstest]
        #[case::unknown_background(None, Color::DarkGray)]
        #[case::dark_without_palette(Some(RgbColor { r: 0x1d, g: 0x20, b: 0x22 }), Color::Indexed(243))]
        #[case::light_without_palette(Some(RgbColor { r: 0xff, g: 0xff, b: 0xff }), Color::Indexed(245))]
        fn swaps_a_bright_black_that_may_not_show_for_a_fixed_grey(
            #[case] background: Option<RgbColor>,
            #[case] expected: Color,
        ) {
            assert_eq!(muted(&HostTheme { background, ..HostTheme::default() }), expected);
        }

        #[test]
        fn keeps_a_bright_black_that_shows() {
            let mut theme =
                HostTheme { background: Some(RgbColor { r: 0x28, g: 0x2a, b: 0x36 }), ..HostTheme::default() };
            theme.palette[8] = Some(RgbColor { r: 0x62, g: 0x72, b: 0xa4 });

            assert_eq!(muted(&theme), Color::DarkGray);
        }
    }

    mod tree_drawing {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

        fn project(name: &str, workspaces: usize, group: Option<usize>, status: Option<Status>) -> ProjectEntry {
            ProjectEntry { name: name.into(), workspaces, group, status }
        }

        fn with_tree() -> View<'static> {
            let shape = TreeShape {
                groups: vec![false],
                tab_bar: false,
                projects: vec![
                    ProjectShape { group: None, collapsed: true, workspaces: Vec::new() },
                    ProjectShape {
                        group: Some(0),
                        collapsed: false,
                        workspaces: vec![
                            WorkspaceShape { collapsed: false, tabs: vec![2, 1] },
                            WorkspaceShape { collapsed: true, tabs: Vec::new() },
                        ],
                    },
                    ProjectShape { group: Some(0), collapsed: true, workspaces: Vec::new() },
                ],
            };
            let details = Details { model: Some("Opus 5.5".into()), percent: Some(23), memory: None };
            let claude = TabEntry { status: Some(Status::Working), details, ..TabEntry::from("claude") };
            let finished = TabEntry { status: Some(Status::Done), ..TabEntry::from("") };
            let workspaces = vec![
                Vec::new(),
                vec![
                    WorkspaceEntry {
                        name: "main".into(),
                        tabs: vec![claude, "zsh".into()],
                        behind: 0,
                        removing: false,
                    },
                    WorkspaceEntry {
                        name: "issue-50-reorder".into(),
                        tabs: vec![finished],
                        behind: 2,
                        removing: false,
                    },
                ],
                Vec::new(),
            ];
            View {
                has_project: true,
                issues: true,
                sidebar: Sidebar::Tree,
                active: 1,
                active_tab: Some(0),
                groups: vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }],
                projects: vec![
                    project("notes", 1, None, None),
                    project("cornercase", 2, Some(0), Some(Status::Done)),
                    project("api", 1, Some(0), Some(Status::Waiting)),
                ],
                tree: Some(TreeView { shape, workspaces }),
                ..view(&[])
            }
        }

        #[test]
        fn grey_text_takes_the_muted_colour() {
            let v = View { muted: Color::Indexed(243), ..with_tree() };
            let t = render_sized(&v, W, TALL.height);
            let a = layout_with(TALL, v.widths, false, Sidebar::Tree);
            let arrow = tree_arrow(a.list, &v.tree.as_ref().expect("a tree").shape, 0, TreeRow::Project(1));
            let first = |r: Rect| {
                let buf = t.backend().buffer();
                (r.x..r.right()).map(|x| &buf[(x, r.y)]).find(|c| !c.symbol().trim().is_empty()).map(|c| c.fg)
            };

            let greys = [a.title, middle(a.search), arrow, a.settings, a.usage, a.quit].map(first);

            assert_eq!(greys, [Some(Color::Indexed(243)); 6]);
        }

        fn row_of(v: &View, row: TreeRow) -> Rect {
            let a = layout_with(TALL, v.widths, false, Sidebar::Tree);
            tree_row(a.list, &v.tree.as_ref().expect("a tree").shape, 0, row)
        }

        fn line(v: &View, row: TreeRow) -> String {
            row_text(&render_sized(v, W, TALL.height), row_of(v, row)).trim_end().to_string()
        }

        #[test]
        fn renders_groups_projects_workspaces_and_tabs() {
            insta::assert_snapshot!(render_sized(&with_tree(), W, TALL.height).backend());
        }

        #[test]
        fn a_narrow_column_cuts_the_names() {
            let v = View { widths: Widths { projects: MIN_COLUMN_WIDTH, ..Widths::default() }, ..with_tree() };
            insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
        }

        #[test]
        fn the_active_tab_holds_the_mark() {
            assert_eq!(line(&with_tree(), TreeRow::Tab(1, 0, 0)), "▌     ├ ◐ claude");
        }

        #[test]
        fn a_folded_project_holds_the_mark_of_its_active_tab() {
            let mut v = with_tree();
            v.tree.as_mut().expect("a tree").shape.projects[1].collapsed = true;
            let text = line(&v, TreeRow::Project(1));
            assert!(text.starts_with("▌   ▸ cornercase (2)"), "{text}");
        }

        #[rstest]
        #[case::a_folded_project(TreeRow::Project(2), "!")]
        #[case::a_folded_workspace(TreeRow::Workspace(1, 1), "✓ ↓2")]
        #[case::an_open_project(TreeRow::Project(1), "cornercase")]
        #[case::an_open_workspace(TreeRow::Workspace(1, 0), "main")]
        fn only_folded_rows_show_what_needs_you(#[case] row: TreeRow, #[case] end: &str) {
            let text = line(&with_tree(), row);
            assert!(text.ends_with(end), "{text}");
        }

        #[rstest]
        #[case::a_tab(TreeRow::Tab(1, 0, 1), Landing { at: 5, spot: Spot::Tab(0) }, 8)]
        #[case::a_workspace(TreeRow::Workspace(1, 1), Landing { at: 4, spot: Spot::Workspace(0) }, 6)]
        #[case::a_project_into_a_group(TreeRow::Project(0), Landing { at: 3, spot: Spot::Project { group: Some(0), before: Some(1) } }, 4)]
        fn the_landing_line_starts_where_the_row_would(
            #[case] row: TreeRow,
            #[case] landing: Landing,
            #[case] indent: usize,
        ) {
            let v = View { drag: Some(Drag::Tree(row, Some(landing))), ..with_tree() };
            let t = render_sized(&v, W, TALL.height);
            let list = layout_with(TALL, v.widths, false, Sidebar::Tree).list;
            let line =
                (list.y..list.bottom()).map(|y| row_text(&t, Rect { y, height: 1, ..list })).find(|l| l.contains('─'));
            assert_eq!(line.map(|l| l.find('─').map(|b| l[..b].chars().count())), Some(Some(indent)));
        }

        #[test]
        fn a_hovered_row_shows_its_close_button() {
            let v = with_tree();
            let r = row_of(&v, TreeRow::Tab(1, 0, 1));
            let t = render_sized(&View { hover: Some(Position::new(r.x + 8, r.y)), ..v }, W, TALL.height);
            assert_eq!(t.backend().buffer()[(r.right() - 2, r.y)].symbol(), "×");
        }
    }

    mod tree_drop {
        use super::*;

        const LIST: Rect = Rect { x: 0, y: 4, width: SIDEBAR_WIDTH - 1, height: 24 };

        fn shape() -> TreeShape {
            let workspace = |tabs: usize| WorkspaceShape { collapsed: false, tabs: vec![1; tabs] };
            TreeShape {
                groups: vec![false],
                tab_bar: false,
                projects: vec![
                    ProjectShape { group: None, collapsed: false, workspaces: vec![workspace(1)] },
                    ProjectShape { group: Some(0), collapsed: false, workspaces: vec![workspace(2), workspace(1)] },
                    ProjectShape { group: Some(0), collapsed: true, workspaces: vec![workspace(1)] },
                ],
            }
        }

        fn drop_on(dragged: TreeRow, under: TreeRow) -> Option<Landing> {
            let r = tree_row(LIST, &shape(), 0, under);
            tree_drop(LIST, &shape(), 0, dragged, Position::new(r.x + 8, r.y))
        }

        fn project(group: Option<usize>, before: Option<usize>) -> Spot {
            Spot::Project { group, before }
        }

        #[rstest]
        #[case::onto_a_tab_of_a_project_above(TreeRow::Project(2), TreeRow::Tab(1, 0, 0), Some((7, project(Some(0), Some(1)))))]
        #[case::after_the_whole_project_below(TreeRow::Project(0), TreeRow::Tab(1, 1, 0), Some((16, project(Some(0), Some(2)))))]
        #[case::to_the_end_of_the_group(TreeRow::Project(1), TreeRow::Project(2), Some((17, project(Some(0), None))))]
        #[case::out_of_its_group(TreeRow::Project(1), TreeRow::Tab(0, 0, 0), Some((0, project(None, Some(0)))))]
        #[case::into_a_group_from_its_header(TreeRow::Project(0), TreeRow::Group(0), Some((7, project(Some(0), Some(1)))))]
        #[case::not_into_itself(TreeRow::Project(1), TreeRow::Tab(1, 0, 0), None)]
        fn a_project_lands_beside_whole_projects(
            #[case] dragged: TreeRow,
            #[case] under: TreeRow,
            #[case] expected: Option<(usize, Spot)>,
        ) {
            assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
        }

        #[rstest]
        #[case::before_a_workspace_above(TreeRow::Workspace(1, 1), TreeRow::Tab(1, 0, 1), Some((8, Spot::Workspace(0))))]
        #[case::after_a_workspace_below(TreeRow::Workspace(1, 0), TreeRow::Tab(1, 1, 0), Some((15, Spot::Workspace(2))))]
        #[case::kept_in_its_project_above(TreeRow::Workspace(1, 1), TreeRow::Tab(0, 0, 0), Some((8, Spot::Workspace(0))))]
        #[case::kept_in_its_project_below(TreeRow::Workspace(1, 0), TreeRow::Project(2), Some((15, Spot::Workspace(2))))]
        fn a_workspace_stays_in_its_project(
            #[case] dragged: TreeRow,
            #[case] under: TreeRow,
            #[case] expected: Option<(usize, Spot)>,
        ) {
            assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
        }

        #[rstest]
        #[case::down_one(TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 0, 1), Some((11, Spot::Tab(2))))]
        #[case::onto_its_workspace(TreeRow::Tab(1, 0, 1), TreeRow::Workspace(1, 0), Some((9, Spot::Tab(0))))]
        #[case::kept_in_its_workspace(TreeRow::Tab(1, 0, 0), TreeRow::Tab(1, 1, 0), Some((11, Spot::Tab(2))))]
        fn a_tab_stays_in_its_workspace(
            #[case] dragged: TreeRow,
            #[case] under: TreeRow,
            #[case] expected: Option<(usize, Spot)>,
        ) {
            assert_eq!(drop_on(dragged, under), expected.map(|(at, spot)| Landing { at, spot }));
        }

        #[test]
        fn a_project_dropped_above_the_list_lands_before_the_project_after_its_own_rows() {
            let short = Rect { height: 8, ..LIST };
            let landing = tree_drop(short, &shape(), 7, TreeRow::Project(1), Position::new(8, short.y - 1));
            assert_eq!(landing, Some(Landing { at: 7, spot: project(Some(0), Some(2)) }));
        }

        #[test]
        fn a_group_moves_with_everything_in_it() {
            assert_eq!(
                drop_on(TreeRow::Group(0), TreeRow::Tab(0, 0, 0)),
                Some(Landing { at: 6, spot: Spot::Group(0) })
            );
        }
    }

    mod sidebar_hit {
        use super::*;

        #[rstest]
        #[case::first(3, 0, Some(SidebarHit::Select(0)))]
        #[case::second(3, 1, Some(SidebarHit::Select(1)))]
        #[case::gap_before_the_new_button(3, 2, None)]
        #[case::new_button_after_two_entries(3, 3, Some(SidebarHit::New))]
        fn maps_rows_to_entries(#[case] x: u16, #[case] row: u16, #[case] expected: Option<SidebarHit>) {
            assert_eq!(sidebar_hit(list(), 1, &plain(2), 0, Position::new(x, list().y + row)), expected);
        }

        #[rstest]
        #[case::first(0)]
        #[case::second(1)]
        fn close_button_closes_its_entry(#[case] i: u16) {
            let pos = Position::new(close_x(), list().y + i);
            assert_eq!(sidebar_hit(list(), 1, &plain(2), 0, pos), Some(SidebarHit::Close(usize::from(i))));
        }

        #[test]
        fn a_narrow_row_leaves_the_menu_to_the_right_click() {
            let narrow = Rect::new(0, 0, MIN_MENU_ROW_WIDTH - 1, 1);
            assert_eq!((row_menu_button(narrow, 1).width, row_close_button(narrow, 1).width), (0, CLOSE_BUTTON_WIDTH));
        }

        #[rstest]
        #[case::wide(1)]
        #[case::compact(COMPACT_PITCH)]
        fn the_menu_button_sits_left_of_the_close_button(#[case] pitch: u16) {
            let row = entry_row(list(), pitch, &plain(2), 0, SidebarRow::Project(1));
            let (menu, close) = (row_menu_button(row, pitch), row_close_button(row, pitch));
            assert_eq!(menu.right(), close.x);
            assert_eq!(sidebar_hit(list(), pitch, &plain(2), 0, menu.as_position()), Some(SidebarHit::Menu(1)));
        }

        #[rstest]
        #[case::search(areas().search.y)]
        #[case::title(areas().title.y)]
        fn header_rows_are_ignored(#[case] y: u16) {
            assert_eq!(sidebar_hit(list(), 1, &plain(1), 0, Position::new(3, y)), None);
        }

        #[test]
        fn pane_is_ignored() {
            assert_eq!(sidebar_hit(list(), 1, &plain(1), 0, Position::new(SIDEBAR_WIDTH + 5, list().y)), None);
        }
    }

    mod groups {
        use super::*;

        const TALL: u16 = 22;

        fn group(name: &str, icon: char, colour: u8) -> GroupEntry {
            GroupEntry { name: name.into(), icon, colour, collapsed: false }
        }

        fn grouped(active: usize, collapsed: bool) -> View<'static> {
            let mut v = View { active, has_project: true, ..view(&["tmp", "api", "web", "cornercase"]) };
            v.groups = vec![group("work", '●', 4), group("oss", '★', 99)];
            v.groups[0].collapsed = collapsed;
            for (p, g) in [(1, 0), (2, 0), (3, 1)] {
                v.projects[p].group = Some(g);
            }
            v
        }

        fn tall_list() -> Rect {
            layout(Rect::new(0, 0, W, TALL), Widths::default()).list
        }

        #[rstest]
        #[case::no_groups(&[None, None], &[], &[SidebarRow::Project(0), SidebarRow::Project(1)])]
        #[case::loose_projects_first(&[Some(0), None], &[false], &[
            SidebarRow::Project(1),
            SidebarRow::Gap,
            SidebarRow::Group(0),
            SidebarRow::Project(0),
        ])]
        #[case::no_gap_without_loose_projects(&[Some(0)], &[false], &[SidebarRow::Group(0), SidebarRow::Project(0)])]
        #[case::collapsed_hides_its_projects(&[Some(0), Some(1)], &[true, false], &[
            SidebarRow::Group(0),
            SidebarRow::Gap,
            SidebarRow::Group(1),
            SidebarRow::Project(1),
        ])]
        #[case::an_empty_group_keeps_its_header(&[None], &[false], &[
            SidebarRow::Project(0),
            SidebarRow::Gap,
            SidebarRow::Group(0),
        ])]
        fn rows(#[case] groups: &[Option<usize>], #[case] collapsed: &[bool], #[case] expected: &[SidebarRow]) {
            assert_eq!(sidebar_rows(groups, collapsed), expected);
        }

        #[rstest]
        #[case::loose_project(0, Some(SidebarHit::Select(0)))]
        #[case::gap(1, None)]
        #[case::header(2, Some(SidebarHit::Group(0)))]
        #[case::grouped_project(3, Some(SidebarHit::Select(1)))]
        fn hit_testing_follows_the_rows(#[case] row: u16, #[case] expected: Option<SidebarHit>) {
            let rows = grouped(0, false).sidebar_rows();
            assert_eq!(sidebar_hit(list(), 1, &rows, 0, Position::new(3, list().y + row)), expected);
        }

        #[rstest]
        #[case::grouped_project(SidebarRow::Project(2), SidebarHit::Close(2))]
        #[case::header(SidebarRow::Group(0), SidebarHit::CloseGroup(0))]
        fn the_close_button_closes_its_row(#[case] row: SidebarRow, #[case] expected: SidebarHit) {
            let rows = grouped(0, false).sidebar_rows();
            let pos = close_button(list(), 1, &rows, 0, row).as_position();
            assert_eq!(sidebar_hit(list(), 1, &rows, 0, pos), Some(expected));
        }

        #[rstest]
        #[case::grouped_project(SidebarRow::Project(2), SidebarHit::Menu(2))]
        #[case::header(SidebarRow::Group(0), SidebarHit::GroupMenu(0))]
        fn the_menu_button_opens_its_row_menu(#[case] row: SidebarRow, #[case] expected: SidebarHit) {
            let rows = grouped(0, false).sidebar_rows();
            let pos = row_menu_button(entry_row(list(), 1, &rows, 0, row), 1).as_position();
            assert_eq!(sidebar_hit(list(), 1, &rows, 0, pos), Some(expected));
        }

        #[test]
        fn renders_expanded_groups_with_the_active_project_inside() {
            insta::assert_snapshot!(render_sized(&grouped(1, false), W, TALL).backend());
        }

        #[test]
        fn a_collapsed_group_holding_the_active_project_marks_its_header() {
            insta::assert_snapshot!(render_sized(&grouped(1, true), W, TALL).backend());
        }

        #[test]
        fn a_collapsed_group_without_the_active_project_is_not_marked() {
            insta::assert_snapshot!(render_sized(&grouped(0, true), W, TALL).backend());
        }

        #[test]
        fn counts_can_be_turned_off() {
            let row_of = |v: &View, row| {
                let r = entry_row(tall_list(), 1, &v.sidebar_rows(), 0, row);
                row_text(&render_sized(v, W, TALL), r)
            };
            let on = grouped(0, true);
            let off = View { counts: false, ..grouped(0, true) };
            let rows = [SidebarRow::Group(0), SidebarRow::Project(0)];
            assert_eq!(rows.map(|row| row_of(&on, row).contains('(')), [true, true]);
            assert_eq!(rows.map(|row| row_of(&off, row).contains('(')), [false, false]);
        }

        #[test]
        fn the_header_takes_the_group_colour() {
            let rows = grouped(0, false).sidebar_rows();
            let header = entry_row(tall_list(), 1, &rows, 0, SidebarRow::Group(1));
            let t = render_sized(&grouped(0, false), W, TALL);
            assert_eq!(t.backend().buffer()[(header.x + 4, header.y)].fg, Color::Indexed(99));
        }

        fn styling(icon: char, colour: u8) -> View<'static> {
            let group = GroupEntry { name: "work".into(), icon, colour, collapsed: false };
            View { overlay: Some(Overlay::GroupStyle(group)), ..grouped(1, false) }
        }

        #[test]
        fn every_preset_is_offered_in_the_modal() {
            assert!(
                GROUP_STYLES.iter().all(|(icon, colour)| GROUP_ICONS.contains(icon) && GROUP_COLOURS.contains(colour))
            );
        }

        #[test]
        fn renders_the_icon_and_colour_modal() {
            insta::assert_snapshot!(render(&styling('★', 99)).backend());
        }

        #[test]
        fn the_chosen_icon_is_highlighted() {
            let t = render(&styling('★', 99));
            let at = |i: usize| t.backend().buffer()[style_icon(AREA, i).as_position()].bg;
            assert_eq!((at(7), at(0)), (Color::Cyan, Color::Reset));
        }

        #[test]
        fn each_swatch_shows_its_colour() {
            let t = render(&styling('★', 99));
            let fg = |i: usize| {
                let cell = style_colour(AREA, i);
                t.backend().buffer()[(cell.x + 1, cell.y)].fg
            };
            assert_eq!((0..GROUP_COLOURS.len()).map(fg).collect::<Vec<_>>(), GROUP_COLOURS.map(Color::Indexed));
        }

        #[rstest]
        #[case::first_icon(style_icon(AREA, 0), Some(StyleHit::Icon(0)))]
        #[case::icon_on_the_second_row(style_icon(AREA, 11), Some(StyleHit::Icon(11)))]
        #[case::colour(style_colour(AREA, 5), Some(StyleHit::Colour(5)))]
        #[case::colour_on_the_second_row(style_colour(AREA, 15), Some(StyleHit::Colour(15)))]
        #[case::done(style_done(AREA), Some(StyleHit::Done))]
        #[case::title(Rect { y: form_area(AREA).y, ..style_icon(AREA, 0) }, None)]
        fn style_hits(#[case] cell: Rect, #[case] expected: Option<StyleHit>) {
            assert_eq!(style_hit(AREA, Position::new(cell.right() - 1, cell.y)), expected);
        }

        #[test]
        fn the_cells_fit_a_narrow_terminal() {
            let narrow = Rect::new(0, 0, 40, 20);
            let all =
                (0..GROUP_ICONS.len()).map(|i| style_icon(narrow, i)).chain((0..16).map(|i| style_colour(narrow, i)));
            assert!(all.into_iter().all(|r| r.width >= ICON_CELL));
        }

        #[test]
        fn the_more_counts_skip_the_gaps() {
            let short = H - 3;
            let t = render_sized(&View { projects_scroll: 2, ..grouped(0, false) }, W, short);
            let above = row_text(&t, more_above(layout(Rect::new(0, 0, W, short), Widths::default()).list));
            assert!(above.contains("↑ 1 more"), "{above:?}");
        }
    }

    mod compact {
        use super::*;

        fn small() -> Areas {
            layout(SMALL, Widths::default())
        }

        fn render_small(view: &View) -> Terminal<TestBackend> {
            let mut t = Terminal::new(TestBackend::new(SMALL.width, SMALL.height)).expect("test backend");
            t.draw(|f| draw(f, view)).expect("draw");
            t
        }

        fn in_a_project(nav: Option<Nav>) -> View<'static> {
            View {
                has_project: true,
                workspaces: vec![
                    WorkspaceEntry {
                        name: "feat/login".into(),
                        tabs: vec!["claude".into(), "nvim".into()],
                        behind: 0,
                        removing: false,
                    },
                    WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, removing: false },
                ],
                active_tab: Some(0),
                nav,
                ..view(&["cornercase", "shop"])
            }
        }

        #[rstest]
        #[case::narrow(COMPACT_WIDTH - 1, true)]
        #[case::wide(COMPACT_WIDTH, false)]
        fn narrow_terminals_get_the_menu_bar(#[case] width: u16, #[case] compact: bool) {
            assert_eq!(layout(Rect::new(0, 0, width, 20), Widths::default()).compact(), compact);
        }

        #[test]
        fn the_projects_menu_draws_in_a_terminal_two_columns_wide() {
            let t = render_sized(&in_a_project(Some(Nav::Projects)), 2, SMALL.height);
            assert_eq!(t.backend().buffer().area.width, 2);
        }

        #[test]
        fn the_pane_takes_everything_under_the_bar() {
            assert_eq!(small().pane, Rect::new(0, COMPACT_PITCH, SMALL.width, SMALL.height - COMPACT_PITCH));
        }

        #[test]
        fn the_search_button_is_the_end_of_the_bar() {
            assert_eq!(
                small().search_button,
                Rect::new(SMALL.width - COMPACT_BUTTON_WIDTH, 0, COMPACT_BUTTON_WIDTH, COMPACT_PITCH)
            );
        }

        #[rstest]
        #[case::closed(None, false, false)]
        #[case::projects(Some(Nav::Projects), true, false)]
        #[case::workspaces(Some(Nav::Workspaces), false, true)]
        fn the_menu_shows_one_column_at_a_time(
            #[case] nav: Option<Nav>,
            #[case] projects: bool,
            #[case] workspaces: bool,
        ) {
            let shown = small().shown(nav);
            assert_eq!((!shown.list.is_empty(), !shown.workspaces_list.is_empty()), (projects, workspaces));
            assert_eq!(!shown.back.is_empty(), workspaces);
        }

        #[test]
        fn wide_terminals_show_both_columns_whatever_the_menu() {
            let areas = layout(AREA, Widths::default());
            assert_eq!(areas.shown(None), areas);
        }

        #[test]
        fn renders_the_bar_over_the_pane() {
            insta::assert_snapshot!(render_small(&in_a_project(None)).backend());
        }

        #[test]
        fn renders_the_workspaces_menu() {
            insta::assert_snapshot!(render_small(&in_a_project(Some(Nav::Workspaces))).backend());
        }

        #[test]
        fn renders_commits_to_pull_in_the_workspaces_menu() {
            let mut v = in_a_project(Some(Nav::Workspaces));
            v.workspaces[0].behind = 12;
            insta::assert_snapshot!(render_small(&v).backend());
        }

        #[test]
        fn renders_the_projects_menu() {
            insta::assert_snapshot!(render_small(&in_a_project(Some(Nav::Projects))).backend());
        }

        fn with_groups(view: View<'static>) -> View<'static> {
            let mut v = View { active: 1, ..view };
            v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
            v.projects[1].group = Some(0);
            v
        }

        #[test]
        fn renders_groups_in_the_projects_menu() {
            insta::assert_snapshot!(render_small(&with_groups(in_a_project(Some(Nav::Projects)))).backend());
        }

        fn close_cell(view: &View, row: Rect) -> (String, Color) {
            let r = row_close_button(row, COMPACT_PITCH);
            let cell = render_small(view).backend().buffer()[Position::new(r.x + r.width / 2, r.y + 1)].clone();
            (cell.symbol().to_string(), cell.fg)
        }

        #[rstest]
        #[case::workspace(WorkspaceRow::Workspace(0))]
        #[case::active_tab(WorkspaceRow::Tab(0, 0))]
        #[case::tab(WorkspaceRow::Tab(0, 1))]
        fn every_workspace_and_tab_shows_its_close_button_without_hover(#[case] row: WorkspaceRow) {
            let v = in_a_project(Some(Nav::Workspaces));
            let r = workspace_row(small().workspaces_list, COMPACT_PITCH, &v.tab_lines(), 0, row);
            assert_eq!(close_cell(&v, r), ("×".into(), Color::DarkGray));
        }

        #[rstest]
        #[case::active(0)]
        #[case::inactive(1)]
        fn every_project_shows_its_close_button_without_hover(#[case] p: usize) {
            let v = in_a_project(Some(Nav::Projects));
            let r = entry_row(small().list, COMPACT_PITCH, &plain(2), 0, SidebarRow::Project(p));
            assert_eq!(close_cell(&v, r), ("×".into(), Color::DarkGray));
        }

        #[test]
        fn a_close_button_turns_red_on_hover() {
            let v = in_a_project(Some(Nav::Workspaces));
            let r = workspace_row(small().workspaces_list, COMPACT_PITCH, &v.tab_lines(), 0, WorkspaceRow::Tab(0, 1));
            let close = row_close_button(r, COMPACT_PITCH);
            let hover = Some(Position::new(close.x + 1, close.y + 1));
            assert_eq!(close_cell(&View { hover, ..v }, r).1, Color::Red);
        }

        #[test]
        fn renders_the_landing_line_between_tabs() {
            let v = in_a_project(Some(Nav::Workspaces));
            let (list, tabs) = (small().workspaces_list, v.tab_lines());
            let nvim = workspace_row(list, COMPACT_PITCH, &tabs, 0, WorkspaceRow::Tab(0, 1));
            let dragged = WorkspaceRow::Tab(0, 0);
            let landing = workspace_drop(list, COMPACT_PITCH, &tabs, 0, dragged, nvim.as_position());
            insta::assert_snapshot!(
                render_small(&View { drag: Some(Drag::Workspaces(dragged, landing)), ..v }).backend()
            );
        }

        #[test]
        fn a_header_is_a_whole_band_to_click() {
            let v = with_groups(in_a_project(Some(Nav::Projects)));
            let rows = v.sidebar_rows();
            let header = entry_row(small().list, COMPACT_PITCH, &rows, 0, SidebarRow::Group(0));
            let bottom = Position::new(header.x + 2, header.bottom() - 1);
            assert_eq!(
                (header.height, sidebar_hit(small().list, COMPACT_PITCH, &rows, 0, bottom)),
                (COMPACT_PITCH, Some(SidebarHit::Group(0)))
            );
        }

        #[test]
        fn the_active_entry_fills_its_whole_band() {
            let t = render_small(&in_a_project(Some(Nav::Projects)));
            let r = entry_row(small().list, COMPACT_PITCH, &plain(2), 0, SidebarRow::Project(0));
            let rows: Vec<Color> = (r.top()..r.bottom()).map(|y| t.backend().buffer()[(r.x + 20, y)].bg).collect();
            assert_eq!(rows, vec![DARK_SURFACE; usize::from(COMPACT_PITCH)]);
        }

        #[test]
        fn the_menu_button_is_a_block_while_the_menu_is_open() {
            let t = render_small(&in_a_project(Some(Nav::Workspaces)));
            let rows: Vec<Color> = (0..COMPACT_PITCH).map(|y| t.backend().buffer()[(1, y)].bg).collect();
            assert_eq!(rows, vec![Color::Cyan; usize::from(COMPACT_PITCH)]);
        }

        #[test]
        fn the_bar_shows_the_brand_without_a_project() {
            let t = render_small(&view(&[]));
            let row: String = (0..SMALL.width).map(|x| t.backend().buffer()[(x, 1)].symbol().to_string()).collect();
            assert!(row.contains("cornercase"), "{row:?}");
        }

        #[test]
        fn the_search_field_takes_the_whole_bar() {
            let search =
                Search { query: "feat".into(), results: Vec::new(), selected: 0, scroll: 0, hint: String::new() };
            let v = View { overlay: Some(Overlay::Search(search)), ..in_a_project(None) };
            let t = render_small(&v);
            let row: String = (0..SMALL.width).map(|x| t.backend().buffer()[(x, 1)].symbol().to_string()).collect();
            assert!(row.starts_with(" ⌕ feat") && !row.contains("claude"), "{row:?}");
        }
    }

    mod sidebar_scroll {
        use super::*;

        fn rows(entries: usize, scroll: usize) -> Rows {
            project_rows(list(), 1, &plain(entries), scroll)
        }

        fn many(n: usize, projects_scroll: usize) -> View<'static> {
            let names: Vec<String> = (0..n).map(|i| format!("p{i}")).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            View { projects_scroll, ..view(&names) }
        }

        #[test]
        fn scrolled_rows_map_to_later_entries() {
            assert_eq!(sidebar_hit(list(), 1, &plain(20), 3, Position::new(3, list().y)), Some(SidebarHit::Select(3)));
        }

        #[test]
        fn the_row_above_the_new_button_is_not_an_entry_when_overflowing() {
            assert_eq!(sidebar_hit(list(), 1, &plain(20), 0, rows(20, 0).more_below().as_position()), None);
        }

        #[rstest]
        #[case::down(0, 3, 3)]
        #[case::stops_at_the_last_entry(0, 100, 12)]
        #[case::up(5, -3, 2)]
        #[case::stops_at_the_first_entry(2, -3, 0)]
        #[case::a_stale_scroll_is_clamped_first(100, -3, 9)]
        fn the_wheel_moves_within_the_entries(#[case] scroll: usize, #[case] delta: isize, #[case] expected: usize) {
            assert_eq!(rows(20, scroll).scrolled(delta), expected);
        }

        #[test]
        fn nothing_scrolls_when_everything_fits() {
            assert_eq!(rows(3, 0).scrolled(3), 0);
        }

        #[rstest]
        #[case::already_visible(0, 2, 0)]
        #[case::below(0, 10, 3)]
        #[case::above(10, 4, 4)]
        fn reveal_scrolls_as_little_as_it_can(#[case] scroll: usize, #[case] i: usize, #[case] expected: usize) {
            assert_eq!(rows(20, scroll).reveal(i), expected);
        }

        #[test]
        fn hidden_entries_are_counted_above_and_below() {
            assert_eq!(rows(20, 3).hidden(), (0..3, 11..20));
        }

        #[test]
        fn shows_how_many_projects_are_hidden() {
            let t = render(&many(20, 3));
            assert_eq!(row_text(&t, more_above(list())).trim_end(), "  ↑ 3 more");
            assert_eq!(row_text(&t, rows(20, 3).more_below()).trim_end(), "  ↓ 9 more");
        }

        #[test]
        fn shows_no_indicator_when_everything_fits() {
            let t = render(&many(3, 0));
            assert_eq!(row_text(&t, more_above(list())).trim(), "");
            assert_eq!(row_text(&t, rows(20, 0).more_below()).trim(), "");
        }

        #[test]
        fn the_new_button_stays_at_the_bottom_while_scrolled() {
            let t = render(&many(20, 3));
            assert!(row_text(&t, new_project_button(list(), 1, &plain(20))).contains(NEW_BUTTON));
        }

        #[test]
        fn workspace_rows_follow_the_scroll() {
            let pos = Position::new(areas().workspaces_list.x + 4, areas().workspaces_list.y);
            assert_eq!(
                workspace_hit(areas().workspaces_list, areas().pitch, &tabs(&[10]), 2, pos),
                Some(WorkspaceHit::Tab(0, 1))
            );
        }

        #[test]
        fn hidden_workspace_rows_count_workspaces_and_tabs_only() {
            let v = View {
                has_project: true,
                workspaces: vec![WorkspaceEntry {
                    name: "main".into(),
                    tabs: vec!["zsh".into(); 10],
                    behind: 0,
                    removing: false,
                }],
                active_tab: Some(0),
                ..view(&["cornercase"])
            };
            let t = render(&v);
            let below = workspace_layout(areas().workspaces_list, 1, &tabs(&[10]), 0).more_below();
            assert_eq!(row_text(&t, below).trim_end(), "  ↓ 3 more");
        }
    }

    mod tall_rows {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 4, width: 40, height: 14 };

        #[test]
        fn each_entry_takes_the_whole_pitch() {
            assert_eq!(project_rows(TALL, 3, &plain(2), 0).item(1), Rect::new(0, 7, 40, 3));
        }

        #[rstest]
        #[case::top(4)]
        #[case::middle(5)]
        #[case::bottom(6)]
        fn any_row_of_an_entry_selects_it(#[case] y: u16) {
            assert_eq!(sidebar_hit(TALL, 3, &plain(2), 0, Position::new(10, y)), Some(SidebarHit::Select(0)));
        }

        #[test]
        fn the_new_button_is_as_tall_as_an_entry() {
            assert_eq!(new_project_button(TALL, 3, &plain(1)), Rect::new(0, 8, 40, 3));
        }

        #[test]
        fn the_gap_between_workspaces_stays_one_row() {
            let second = workspace_row(TALL, 3, &tabs(&[0, 0]), 0, WorkspaceRow::Workspace(1));
            assert_eq!(second.y, TALL.y + 3 + 3 + GAP);
        }

        #[test]
        fn the_wheel_moves_by_entries() {
            assert_eq!(project_rows(TALL, 3, &plain(10), 0).scrolled(100), 7);
        }

        #[test]
        fn reveal_counts_rows_not_entries() {
            assert_eq!(project_rows(TALL, 3, &plain(10), 0).reveal(5), 3);
        }

        #[test]
        fn a_tall_row_gets_a_wider_close_button() {
            assert_eq!(row_close_button(Rect::new(0, 0, 40, 3), 3).width, COMPACT_CLOSE_WIDTH);
        }
    }

    mod buttons {
        use super::*;

        #[test]
        fn new_button_sticks_to_bottom_when_entries_overflow() {
            assert_eq!(new_project_button(list(), 1, &plain(100)).y, list().bottom() - 1);
        }

        #[test]
        fn new_button_wins_over_entries_when_overflowing() {
            let pos = Position::new(3, list().bottom() - 1);
            assert_eq!(sidebar_hit(list(), 1, &plain(100), 0, pos), Some(SidebarHit::New));
        }

        #[test]
        fn close_button_out_of_view_is_empty() {
            assert!(close_button(list(), 1, &plain(50), 0, SidebarRow::Project(49)).is_empty());
        }

        fn with_update(hover: Option<Position>) -> View<'static> {
            View { update: Some("↑ 9.0.0".into()), hover, ..view(&["~"]) }
        }

        fn settings_row(v: &View) -> String {
            let t = render(v);
            let r = areas().settings;
            (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
        }

        #[test]
        fn an_update_sits_at_the_end_of_the_settings_row() {
            let row = settings_row(&with_update(None));
            assert!(row.starts_with("  Settings ") && row.ends_with(" ↑ 9.0.0 "), "{row:?}");
        }

        #[test]
        fn hovering_the_update_leaves_settings_alone() {
            let r = update_button(areas().settings, "↑ 9.0.0");
            let t = render(&with_update(Some(r.as_position())));
            let buffer = t.backend().buffer();
            let settings = areas().settings.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 });
            assert_eq!((buffer[r.as_position()].bg, buffer[settings].bg), (Color::Cyan, Color::Reset));
        }
    }

    mod workspaces_column {
        use super::*;

        fn wlist() -> Rect {
            areas().workspaces_list
        }

        fn at(row: u16) -> Position {
            Position::new(wlist().x + 4, wlist().y + row)
        }

        fn close_at(row: u16) -> Position {
            Position::new(wlist().right() - 2, wlist().y + row)
        }

        fn with_workspaces(active_tab: Option<usize>) -> View<'static> {
            View {
                has_project: true,
                workspaces: vec![
                    WorkspaceEntry {
                        name: "login".into(),
                        tabs: vec!["claude".into(), "nvim".into()],
                        behind: 0,
                        removing: false,
                    },
                    WorkspaceEntry { name: "main".into(), tabs: vec!["zsh".into()], behind: 0, removing: false },
                ],
                active_tab,
                ..view(&["cornercase"])
            }
        }

        fn row_text(v: &View, row: WorkspaceRow) -> String {
            let r = workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), v.workspaces_scroll, row);
            let t = render(v);
            (r.x..r.right()).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect()
        }

        #[rstest]
        #[case::workspace(WorkspaceRow::Workspace(0))]
        #[case::tab(WorkspaceRow::Tab(0, 0))]
        fn long_names_keep_their_start(#[case] row: WorkspaceRow) {
            let long = "feature-with-a-very-long-name-that-does-not-fit";
            let mut v = with_workspaces(Some(0));
            v.workspaces[0].name = long.into();
            v.workspaces[0].tabs[0] = long.into();
            let text = row_text(&v, row);
            assert!(text.contains("feature-with-") && text.contains('…'), "{text:?}");
        }

        #[test]
        fn rows_are_each_workspace_then_its_tabs_then_plus_tab() {
            use WorkspaceRow::{Gap, NewTab, Tab, Workspace};
            assert_eq!(
                workspace_rows(&tabs(&[2, 0])),
                [Workspace(0), Tab(0, 0), Tab(0, 1), NewTab(0), Gap, Workspace(1), NewTab(1)]
            );
        }

        #[rstest]
        #[case::workspace(at(0), Some(WorkspaceHit::Workspace(0)))]
        #[case::workspace_close(close_at(0), Some(WorkspaceHit::CloseWorkspace(0)))]
        #[case::tab(at(1), Some(WorkspaceHit::Tab(0, 0)))]
        #[case::tab_close(close_at(2), Some(WorkspaceHit::CloseTab(0, 1)))]
        #[case::plus_tab(at(3), Some(WorkspaceHit::NewTab(0)))]
        #[case::gap_before_new_workspace(at(4), None)]
        #[case::new_workspace(at(5), Some(WorkspaceHit::NewWorkspace))]
        fn maps_clicks(#[case] pos: Position, #[case] expected: Option<WorkspaceHit>) {
            assert_eq!(workspace_hit(wlist(), 1, &tabs(&[2]), 0, pos), expected);
        }

        #[test]
        fn the_gap_between_workspaces_is_not_a_row() {
            assert_eq!(workspace_hit(wlist(), 1, &tabs(&[0, 0]), 0, at(2)), None);
        }

        #[test]
        fn new_workspace_sticks_to_the_bottom_when_rows_overflow() {
            assert_eq!(new_workspace_button(wlist(), 1, &tabs(&[100])).y, wlist().bottom() - 1);
        }

        #[test]
        fn rows_under_the_new_workspace_button_are_hidden() {
            let last_visible = usize::from(wlist().height) - 1;
            assert!(workspace_row(wlist(), 1, &tabs(&[100]), 0, WorkspaceRow::Tab(0, last_visible)).is_empty());
        }

        #[test]
        fn renders_the_workspaces_of_the_active_project() {
            insta::assert_snapshot!(render(&with_workspaces(Some(0))).backend());
        }

        #[test]
        fn renders_commits_to_pull_before_the_close_button() {
            let mut v = View { hover: Some(at(0)), ..with_workspaces(Some(0)) };
            v.workspaces[0].behind = 3;
            v.workspaces[1].behind = 1;
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn renders_a_workspace_being_removed_without_its_buttons() {
            let mut v = View { hover: Some(at(0)), active_workspace: 1, ..with_workspaces(Some(0)) };
            v.workspaces[0] = WorkspaceEntry { behind: 3, removing: true, tabs: Vec::new(), name: "login".into() };
            insta::assert_snapshot!(render(&v).backend());
        }

        #[rstest]
        #[case::nothing_to_pull("login", 0, "  login")]
        #[case::some("login", 3, "  login          ↓3")]
        #[case::long_name_is_cut_first("feature-with-a-very-long-name", 3, "  feature-with-… ↓3")]
        fn commits_to_pull_sit_at_the_end_of_the_row(#[case] name: &str, #[case] behind: u32, #[case] expected: &str) {
            let mut v = with_workspaces(Some(0));
            v.workspaces[0] = WorkspaceEntry { name: name.into(), behind, ..v.workspaces.remove(0) };
            assert_eq!(row_text(&v, WorkspaceRow::Workspace(0)).trim_end(), expected);
        }

        #[test]
        fn marks_the_active_tab() {
            let pos = Position::new(wlist().x, wlist().y + 1);
            assert_eq!(render(&with_workspaces(Some(0))).backend().buffer()[pos].symbol(), "▌");
        }

        #[test]
        fn plus_tab_is_filled_on_hover() {
            let v = View { hover: Some(at(3)), ..with_workspaces(Some(0)) };
            assert_eq!(render(&v).backend().buffer()[at(3)].bg, Color::Cyan);
        }

        #[test]
        fn an_empty_workspace_says_so_in_the_pane() {
            let v = View {
                workspaces: vec![WorkspaceEntry { name: "main".into(), tabs: Vec::new(), behind: 0, removing: false }],
                ..with_workspaces(None)
            };
            let text: String =
                render(&v).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
            assert!(text.contains("no tab open"), "{text}");
        }

        #[test]
        fn nothing_shows_without_a_project() {
            let text: String =
                render(&view(&[])).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
            assert!(!text.contains("New workspace"), "{text}");
        }
    }

    mod agent_status {
        use super::*;

        fn tab(name: &str, status: Option<Status>) -> TabEntry {
            TabEntry { status, ..name.into() }
        }

        fn with_agents() -> View<'static> {
            let mut v = View {
                has_project: true,
                workspaces: vec![
                    WorkspaceEntry {
                        name: "login".into(),
                        tabs: vec![
                            tab("claude", Some(Status::Working)),
                            tab("claude", Some(Status::Waiting)),
                            "nvim".into(),
                        ],
                        behind: 2,
                        removing: false,
                    },
                    WorkspaceEntry {
                        name: "main".into(),
                        tabs: vec![tab("claude", Some(Status::Done)), tab("claude", Some(Status::Idle))],
                        behind: 0,
                        removing: false,
                    },
                ],
                active_tab: Some(0),
                ..view(&["shop", "api", "web", "docs"])
            };
            v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: true }];
            for (p, status) in [(0, Status::Waiting), (1, Status::Done), (3, Status::Waiting)] {
                v.projects[p].status = Some(status);
            }
            for p in [2, 3] {
                v.projects[p].group = Some(0);
            }
            v
        }

        fn tab_row(v: &View, w: usize, t: usize) -> Rect {
            workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Tab(w, t))
        }

        fn workspace_line(v: &View, w: usize) -> String {
            let r =
                workspace_row(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, WorkspaceRow::Workspace(w));
            row_text(&render(v), r).trim_end().to_string()
        }

        fn sidebar_row(v: &View, row: SidebarRow) -> Rect {
            let sidebar = v.sidebar_rows();
            let i = sidebar.iter().position(|r| *r == row).expect("the row is in the sidebar");
            project_rows(list(), areas().pitch, &sidebar, 0).item(i)
        }

        fn mark_cell(v: &View, r: Rect) -> (String, Color) {
            let t = render(v);
            let cell = &t.backend().buffer()[(r.right() - CLOSE_BUTTON_WIDTH - MENU_BUTTON_WIDTH - 2, r.y)];
            (cell.symbol().to_string(), cell.fg)
        }

        #[test]
        fn renders_icons_on_tabs_and_marks_on_the_rows_above() {
            insta::assert_snapshot!(render(&with_agents()).backend());
        }

        #[rstest]
        #[case::working(Status::Working, "◐", Color::Yellow)]
        #[case::waiting(Status::Waiting, "!", WAITING_COLOR)]
        #[case::done(Status::Done, "✓", Color::Green)]
        #[case::idle(Status::Idle, "○", Color::DarkGray)]
        fn a_tab_shows_its_agent_before_the_name(#[case] status: Status, #[case] icon: &str, #[case] colour: Color) {
            let mut v = with_agents();
            v.workspaces[0].tabs[1] = tab("claude", Some(status));
            let r = tab_row(&v, 0, 1);
            let t = render(&v);
            let cell = &t.backend().buffer()[(r.x + 4, r.y)];
            assert_eq!((row_text(&t, r).trim_end().to_string(), cell.fg), (format!("  ├ {icon} claude"), colour));
        }

        #[test]
        fn a_tab_without_an_agent_has_no_icon() {
            let v = with_agents();
            assert_eq!(row_text(&render(&v), tab_row(&v, 0, 2)).trim_end(), "  ├ nvim");
        }

        #[test]
        fn a_split_tab_shows_how_many_other_panes_it_has_at_the_end_of_its_row() {
            let mut v = with_agents();
            v.workspaces[0].tabs[2].others = 2;
            let r = tab_row(&v, 0, 2);
            let t = render(&v);
            let end = r.right() - CLOSE_BUTTON_WIDTH - MENU_BUTTON_WIDTH - 2;
            let count: String = (end - 1..=end).map(|x| t.backend().buffer()[(x, r.y)].symbol().to_string()).collect();
            let text = row_text(&t, r).trim_end().to_string();
            assert_eq!(
                (text.starts_with("  ├ nvim "), count.as_str(), t.backend().buffer()[(end, r.y)].fg),
                (true, "+2", Color::DarkGray),
                "{text}"
            );
        }

        #[test]
        fn a_long_name_is_cut_before_the_count_is() {
            let mut v = with_agents();
            v.workspaces[0].tabs[1] = TabEntry { others: 1, ..tab("a-very-long-program-name", Some(Status::Working)) };
            let text = row_text(&render(&v), tab_row(&v, 0, 1)).trim_end().to_string();
            assert!(text.contains('…') && text.ends_with(" +1"), "{text}");
        }

        #[test]
        fn a_row_too_narrow_for_the_name_still_shows_the_count() {
            let v = with_agents();
            let r = Rect::new(0, 0, 12, 1);
            let band = Band { r, pitch: 1, lead: vec![Span::raw("  ├ ")], bg: Style::default() };
            let tab = TabEntry { others: 1, .."claude".into() };
            let mut t = Terminal::new(TestBackend::new(r.width, 1)).expect("test backend");
            t.draw(|f| draw_tab_band(f, &v, band, &tab, false)).expect("draw");
            assert_eq!(row_text(&t, r).trim_end(), "  ├ c +1");
        }

        #[test]
        fn a_tab_with_one_pane_shows_no_count() {
            let v = with_agents();
            assert!(!row_text(&render(&v), tab_row(&v, 0, 2)).contains('+'));
        }

        #[test]
        fn the_workspace_mark_comes_before_the_commits_to_pull() {
            assert_eq!(workspace_line(&with_agents(), 0), "  login        ! ↓2");
        }

        #[test]
        fn only_what_needs_you_reaches_the_workspace() {
            let mut v = with_agents();
            v.workspaces[0].tabs = vec![tab("claude", Some(Status::Working)), tab("claude", Some(Status::Idle))];
            assert_eq!(workspace_line(&v, 0), "  login          ↓2");
        }

        #[test]
        fn done_waits_behind_what_needs_you() {
            let mut v = with_agents();
            v.workspaces[0].tabs = vec![tab("claude", Some(Status::Done)), tab("claude", Some(Status::Waiting))];
            assert_eq!(workspace_line(&v, 0), "  login        ! ↓2");
        }

        #[rstest]
        #[case::waiting(0, ("!", WAITING_COLOR))]
        #[case::done(1, ("✓", Color::Green))]
        fn a_project_shows_its_mark_at_the_end_of_the_row(#[case] p: usize, #[case] expected: (&str, Color)) {
            let v = with_agents();
            let (symbol, fg) = mark_cell(&v, sidebar_row(&v, SidebarRow::Project(p)));
            assert_eq!((symbol.as_str(), fg), expected);
        }

        #[test]
        fn a_collapsed_group_shows_the_mark_of_its_projects() {
            let v = with_agents();
            let (symbol, fg) = mark_cell(&v, sidebar_row(&v, SidebarRow::Group(0)));
            assert_eq!((symbol.as_str(), fg), ("!", WAITING_COLOR));
        }

        #[test]
        fn an_expanded_group_leaves_the_mark_to_its_projects() {
            let mut v = with_agents();
            v.groups[0].collapsed = false;
            v.projects[2].status = Some(Status::Waiting);
            let (group, _) = mark_cell(&v, sidebar_row(&v, SidebarRow::Group(0)));
            let (project, _) = mark_cell(&v, sidebar_row(&v, SidebarRow::Project(2)));
            assert_eq!((group.as_str(), project.as_str()), (" ", "!"));
        }

        #[rstest]
        #[case::nothing(None, "   ≡   ")]
        #[case::done_elsewhere(Some(Status::Done), "   ≡ ✓ ")]
        #[case::waiting_elsewhere(Some(Status::Waiting), "   ≡ ! ")]
        fn the_menu_button_says_when_another_tab_needs_you(#[case] attention: Option<Status>, #[case] expected: &str) {
            let v = View { attention, ..with_agents() };
            let mut t = Terminal::new(TestBackend::new(SMALL.width, SMALL.height)).expect("test backend");
            t.draw(|f| draw(f, &v)).expect("draw");
            assert_eq!(row_text(&t, Rect::new(0, 1, COMPACT_BUTTON_WIDTH, 1)), expected);
        }
    }

    mod tab_context {
        use super::*;

        fn opus(percent: u16) -> Details {
            Details { model: Some("Opus 5.5".into()), percent: Some(percent), memory: None }
        }

        fn with_details(details: Details) -> View<'static> {
            let claude = TabEntry { status: Some(Status::Working), details, ..TabEntry::from("claude") };
            View {
                has_project: true,
                workspaces: vec![WorkspaceEntry {
                    name: "login".into(),
                    tabs: vec![claude, "nvim".into()],
                    behind: 0,
                    removing: false,
                }],
                active_tab: Some(0),
                ..view(&["shop"])
            }
        }

        fn row(v: &View, area: Rect, row: WorkspaceRow) -> Rect {
            let a = layout(area, v.widths);
            workspace_row(a.workspaces_list, a.pitch, &v.tab_lines(), 0, row)
        }

        fn line(t: &Terminal<TestBackend>, r: Rect, y: u16) -> String {
            row_text(t, Rect { y: r.y + y, height: 1, ..r }).trim_end().to_string()
        }

        #[test]
        fn renders_the_model_and_the_context_under_the_tab() {
            insta::assert_snapshot!(render(&with_details(opus(17))).backend());
        }

        fn agent(name: &str, model: &str, percent: Option<u16>) -> View<'static> {
            let mut view = with_details(Details { model: Some(model.into()), percent, memory: None });
            let tab = &mut view.workspaces[0].tabs[0];
            tab.name = name.into();
            tab.status = None;
            view
        }

        fn codex(model: &str, percent: Option<u16>) -> View<'static> {
            agent("codex", model, percent)
        }

        #[test]
        fn renders_codex_context_in_the_wide_layout() {
            insta::assert_snapshot!(render(&codex("gpt-5.4", Some(20))).backend());
        }

        #[test]
        fn renders_opencode_working_with_its_model_and_context() {
            let mut view = agent("opencode", "DeepSeek V4 Pro", Some(12));
            view.workspaces[0].tabs[0].status = Some(Status::Working);

            insta::assert_snapshot!(render(&view).backend());
        }

        #[test]
        fn renders_codex_without_a_percentage_in_the_compact_layout() {
            let view = View { nav: Some(Nav::Workspaces), ..codex("gpt-5.4-mini", None) };

            insta::assert_snapshot!(render_sized(&view, SMALL.width, SMALL.height).backend());
        }

        #[rstest]
        #[case::wide(AREA)]
        #[case::compact(SMALL)]
        #[case::narrow(Rect::new(0, 0, 30, 25))]
        fn long_codex_model_names_are_truncated_and_keep_the_percentage(#[case] area: Rect) {
            let model = "a-very-long-codex-model-name".repeat(5);
            let view = View { nav: Some(Nav::Workspaces), ..codex(&model, Some(91)) };
            let r = row(&view, area, WorkspaceRow::Tab(0, 0));
            let rendered = render_sized(&view, area.width, area.height);
            let text = line(&rendered, r, r.height - 1);

            assert!(text.contains("91%"), "{text}");
            assert!(!text.contains(&model), "{text}");
        }

        #[test]
        fn a_model_without_usage_is_truncated_in_the_wide_layout() {
            let view = codex("a-very-long-codex-model-name", None);
            let r = row(&view, AREA, WorkspaceRow::Tab(0, 0));
            let rendered = render(&view);
            let text = line(&rendered, r, 1);

            assert!(text.contains('…'), "{text}");
            assert!(!text.contains('%') && !text.contains('·'), "{text}");
        }

        #[test]
        fn the_tab_takes_a_second_row() {
            assert_eq!(row(&with_details(opus(17)), AREA, WorkspaceRow::Tab(0, 0)).height, 2);
        }

        #[test]
        fn the_rows_below_move_down_one() {
            let moved = row(&with_details(opus(17)), AREA, WorkspaceRow::Tab(0, 1));
            let before = row(&with_details(Details::default()), AREA, WorkspaceRow::Tab(0, 1));
            assert_eq!(moved.y, before.y + 1);
        }

        #[rstest]
        #[case::fits(26, None, "▌ │   Opus 5.5 · 17%")]
        #[case::cuts_the_model(19, None, "▌ │   Opus… · 17%")]
        #[case::keeps_the_percentage(16, None, "▌ │   17%")]
        #[case::fits_with_the_memory(32, Some(GB * 12 / 10), "▌ │   Opus 5.5 · 17% · 1.2 GB")]
        #[case::cuts_the_model_before_the_memory(30, Some(GB * 12 / 10), "▌ │   Opus 5… · 17% · 1.2 GB")]
        #[case::drops_the_model_before_the_memory(26, Some(GB * 12 / 10), "▌ │   17% · 1.2 GB")]
        #[case::moves_left_to_keep_the_memory(16, Some(GB * 12 / 10), "▌ 17% · 1.2 GB")]
        fn a_narrow_column_cuts_the_model_first(
            #[case] workspaces: u16,
            #[case] memory: Option<u64>,
            #[case] expected: &str,
        ) {
            let v = View {
                widths: Widths { workspaces, ..Widths::default() },
                ..with_details(Details { memory, ..opus(17) })
            };
            let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
            assert_eq!(line(&render(&v), r, 1), expected);
        }

        #[rstest]
        #[case::the_model(Details { model: Some("Opus 5.5".into()), ..Details::default() }, "▌ │   Opus 5.5")]
        #[case::the_context(Details { percent: Some(17), ..Details::default() }, "▌ │   17%")]
        #[case::the_memory(Details { memory: Some(300 * MB), ..Details::default() }, "▌ │   300 MB")]
        #[case::the_model_and_the_memory(Details { memory: Some(GB * 12 / 10), ..codex_model() }, "▌ │   gpt-5.4 · 1.2 GB")]
        #[case::the_context_and_the_memory(Details { model: None, memory: Some(300 * MB), ..opus(17) }, "▌ │   17% · 300 MB")]
        fn each_part_shows_without_the_others(#[case] details: Details, #[case] expected: &str) {
            let v = with_details(details);
            let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));

            assert_eq!((r.height, line(&render(&v), r, 1)), (2, expected.into()));
        }

        fn codex_model() -> Details {
            Details { model: Some("gpt-5.4".into()), ..Details::default() }
        }

        #[test]
        fn renders_the_memory_in_the_compact_layout() {
            let view =
                View { nav: Some(Nav::Workspaces), ..with_details(Details { memory: Some(GB * 12 / 10), ..opus(17) }) };

            insta::assert_snapshot!(render_sized(&view, SMALL.width, SMALL.height).backend());
        }

        #[rstest]
        #[case::megabytes(300 * MB, "300 MB")]
        #[case::rounded_to_the_nearest_megabyte(300 * MB + MB * 6 / 10, "301 MB")]
        #[case::nearly_a_gigabyte(GB - MB / 10, "1.0 GB")]
        #[case::gigabytes(GB * 12 / 10, "1.2 GB")]
        #[case::rounded_to_a_tenth(GB * 25 / 2, "12.5 GB")]
        fn memory_reads_in_megabytes_then_gigabytes(#[case] bytes: u64, #[case] expected: &str) {
            assert_eq!(memory_size(bytes), expected);
        }

        #[rstest]
        #[case::normal(40, Color::DarkGray)]
        #[case::getting_full(80, WAITING_COLOR)]
        #[case::nearly_full(95, Color::Red)]
        fn the_percentage_turns_orange_then_red(#[case] percent: u16, #[case] colour: Color) {
            let v = with_details(opus(percent));
            let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
            let t = render(&v);
            let end = u16::try_from(line(&t, r, 1).chars().count()).expect("fits");
            assert_eq!(t.backend().buffer()[(r.x + end - 1, r.y + 1)].fg, colour);
        }

        #[test]
        fn only_the_first_row_has_the_close_button() {
            let v = with_details(opus(17));
            let r = row(&v, AREA, WorkspaceRow::Tab(0, 0));
            let hit = |x: u16, y: u16| {
                workspace_hit(areas().workspaces_list, areas().pitch, &v.tab_lines(), 0, Position::new(x, y))
            };
            let menu = row_menu_button(r, areas().pitch);
            assert_eq!(
                [hit(r.x + 4, r.y + 1), hit(r.right() - 2, r.y + 1), hit(r.right() - 2, r.y), hit(menu.x, menu.y)],
                [
                    Some(WorkspaceHit::Tab(0, 0)),
                    Some(WorkspaceHit::Tab(0, 0)),
                    Some(WorkspaceHit::CloseTab(0, 0)),
                    Some(WorkspaceHit::TabMenu(0, 0))
                ]
            );
        }

        #[test]
        fn the_close_button_covers_only_the_first_band_of_a_taller_row() {
            assert_eq!(row_close_button(Rect::new(0, 4, 25, 2), 1), Rect::new(22, 4, CLOSE_BUTTON_WIDTH, 1));
        }

        #[test]
        fn a_narrow_compact_band_keeps_the_percentage_clear_of_the_buttons() {
            let narrow = Rect { width: 34, ..SMALL };
            let details = Details { model: Some("Opus 5.5 with a long name".into()), ..opus(95) };
            let v = View { nav: Some(Nav::Workspaces), ..with_details(details) };
            let r = row(&v, narrow, WorkspaceRow::Tab(0, 0));
            let t = render_sized(&v, narrow.width, narrow.height);
            let end = u16::try_from(line(&t, r, 2).chars().count()).expect("fits");
            assert!(r.x + end <= row_menu_button(r, COMPACT_PITCH).x, "{}", line(&t, r, 2));
        }

        #[test]
        fn a_compact_band_shows_it_on_its_last_row() {
            let v = View { nav: Some(Nav::Workspaces), ..with_details(opus(17)) };
            let r = row(&v, SMALL, WorkspaceRow::Tab(0, 0));
            let t = render_sized(&v, SMALL.width, SMALL.height);
            assert_eq!(
                (r.height, line(&t, r, 1), line(&t, r, 2)),
                (
                    COMPACT_PITCH,
                    format!("▌ ├ ◐ claude{}{ROW_MENU_ICON}  ×", " ".repeat(usize::from(SMALL.width) - 18)),
                    "▌ │   Opus 5.5 · 17%".into()
                )
            );
        }
    }

    mod dialogs {
        use super::*;

        fn new_workspace_form(on: bool) -> Form {
            Form {
                title: "New workspace",
                label: "name",
                value: "feat/login".into(),
                hint: "in ~/.cornercase/worktrees/cornercase/feat-login".into(),
                toggle: Some(Toggle { label: "with its own worktree", on }),
                note: None,
                submit: "create",
            }
        }

        fn with(overlay: Overlay) -> View<'static> {
            View { overlay: Some(overlay), ..view(&["cornercase"]) }
        }

        #[test]
        fn renders_the_worktree_toggle() {
            insta::assert_snapshot!(render(&with(Overlay::Form(new_workspace_form(true)))).backend());
        }

        #[test]
        fn the_toggle_row_is_hit() {
            let pos = form_toggle(form_area(AREA)).as_position();
            assert_eq!(form_hit(AREA, "create", pos), Some(FormHit::Toggle));
        }

        fn usage_window(label: &str, percent: u16, severity: Severity, resets: &str) -> UsageWindow {
            UsageWindow { label: label.into(), percent, severity, resets: resets.into() }
        }

        fn claude_usage() -> UsageSection {
            UsageSection {
                title: "Claude Code · max plan".into(),
                status: "updated 2m ago".into(),
                error: None,
                windows: vec![
                    usage_window("session (5h)", 7, Severity::Normal, "resets in 2h 13m"),
                    usage_window("week", 82, Severity::Warning, "resets in 3d 4h"),
                    usage_window("week · Fable", 100, Severity::Critical, ""),
                ],
                empty: None,
                extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
            }
        }

        fn codex_usage() -> UsageSection {
            UsageSection {
                title: "Codex · plus plan".into(),
                status: "updated just now".into(),
                error: None,
                windows: vec![
                    usage_window("session (5h)", 53, Severity::Normal, "resets in 4h 59m"),
                    usage_window("week", 8, Severity::Normal, "resets in 5d 22h"),
                ],
                empty: None,
                extra: None,
            }
        }

        fn usage() -> Usage {
            Usage { sections: vec![claude_usage()] }
        }

        const TALL: u16 = 34;

        #[test]
        fn renders_the_usage_windows() {
            insta::assert_snapshot!(render(&with(Overlay::Usage(usage()))).backend());
        }

        #[test]
        fn renders_one_section_per_agent() {
            let usage = Usage { sections: vec![claude_usage(), codex_usage()] };
            insta::assert_snapshot!(render_sized(&with(Overlay::Usage(usage)), W, TALL).backend());
        }

        #[test]
        fn renders_usage_while_loading_and_after_a_failure() {
            let loading =
                UsageSection { status: "loading…".into(), windows: Vec::new(), extra: None, ..claude_usage() };
            let failed = UsageSection {
                title: "Codex".into(),
                status: String::new(),
                error: Some(
                    "usage unavailable: could not run /opt/codex/bin/codex: No such file or directory (os error 2)"
                        .into(),
                ),
                windows: Vec::new(),
                ..codex_usage()
            };
            let usage = Usage { sections: vec![loading, failed] };
            insta::assert_snapshot!(render(&with(Overlay::Usage(usage))).backend());
        }

        #[rstest]
        #[case::normal(0, Color::Green)]
        #[case::warning(1, Color::Indexed(208))]
        #[case::critical(2, Color::Red)]
        fn usage_bars_take_the_severity_colour(#[case] window: u16, #[case] expected: Color) {
            let t = render(&with(Overlay::Usage(usage())));
            let body = usage_rows(usage_area(AREA, &usage()))[0];
            let bar = Position::new(body.x, body.y + 3 + window * 3);
            assert_eq!(t.backend().buffer()[bar].fg, expected);
        }

        #[test]
        fn the_rest_of_a_usage_bar_takes_the_line_colour() {
            let t = render(&with(Overlay::Usage(usage())));
            let body = usage_rows(usage_area(AREA, &usage()))[0];
            let bar = Position::new(body.right() - 1, body.y + 3);
            assert_eq!(t.backend().buffer()[bar].fg, DARK_LINE);
        }

        #[test]
        fn a_dialog_dims_what_is_behind_it() {
            let t = render(&with(Overlay::Form(new_workspace_form(true))));
            let form = form_area(AREA);
            let buffer = t.backend().buffer();
            let behind = buffer[areas().title.as_position()].modifier.contains(Modifier::DIM);
            let inside = buffer[form_rows(form)[1].as_position()].modifier.contains(Modifier::DIM);
            assert_eq!((behind, inside), (true, false));
        }

        #[test]
        fn the_usage_done_button_sits_on_the_last_row() {
            let r = usage_area(AREA, &usage());
            assert_eq!(usage_done(AREA, &usage()).y, r.bottom() - 2);
        }

        #[test]
        fn a_usage_too_tall_for_the_screen_keeps_its_done_button() {
            let usage = Usage { sections: vec![claude_usage(), codex_usage()] };
            let r = usage_area(AREA, &usage);
            assert_eq!((r.height, usage_done(AREA, &usage).y), (AREA.height, r.bottom() - 2));
        }

        #[test]
        fn renders_a_confirmation() {
            let confirm = Confirm {
                title: "Remove workspace",
                message: "Remove the workspace login and delete its worktree folder ~/wt/login? The branch is kept."
                    .into(),
                note: Some(Note::Error("contains modified or untracked files, use --force to delete it".into())),
                submit: "remove anyway",
            };
            insta::assert_snapshot!(render(&with(Overlay::Confirm(confirm))).backend());
        }

        #[test]
        fn renders_an_update_with_its_notes() {
            let notes = (1..=40).map(|i| Line::from(format!("• change {i}"))).collect();
            let update = Update {
                title: "Update",
                message: "cornercase 9.0.0 is out (you have 0.1.0). Updating replaces ~/.local/bin/cornercase; \
                    your terminals keep running until you restart."
                    .into(),
                notes,
                scroll: 30,
                note: Some(Note::Busy("downloading…")),
                submit: "update",
                cancel: CANCEL_LABEL,
            };
            insta::assert_snapshot!(render(&with(Overlay::Update(update))).backend());
        }
    }

    mod truncate_left {
        use super::*;

        #[rstest]
        #[case::fits("short", 10, "short")]
        #[case::keeps_the_end("/a/b/c/project", 8, "…project")]
        #[case::counts_chars_not_bytes("ñandú/añil", 5, "…añil")]
        #[case::keeps_a_single_ellipsis("x › …/b/project", 12, "…/b/project")]
        fn shortens_from_the_left(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
            assert_eq!(truncate_left(input, max), expected);
        }
    }

    mod truncate_right {
        use super::*;

        #[rstest]
        #[case::fits("short", 10, "short")]
        #[case::keeps_the_start("feature/login-page", 8, "feature…")]
        #[case::counts_chars_not_bytes("ñandú/añil", 5, "ñand…")]
        #[case::keeps_a_single_ellipsis("shop › #482 empty addr… › bash", 24, "shop › #482 empty addr…")]
        fn shortens_from_the_right(#[case] input: &str, #[case] max: usize, #[case] expected: &str) {
            assert_eq!(truncate_right(input, max), expected);
        }
    }

    mod folder_name {
        use super::*;

        #[rstest]
        #[case::last_component("/home/ana/projects/cornercase", Some("/home/ana"), "cornercase")]
        #[case::home_itself("/home/ana", Some("/home/ana"), "~")]
        #[case::no_home("/tmp", None, "tmp")]
        #[case::root("/", Some("/home/ana"), "/")]
        fn shows_only_the_current_folder(#[case] path: &str, #[case] home: Option<&str>, #[case] expected: &str) {
            assert_eq!(folder_name(Path::new(path), home.map(Path::new)), expected);
        }
    }

    mod draw {
        use super::*;

        #[test]
        fn renders_sidebar_with_active_entry() {
            let v = View { active: 1, ..view(&["cornercase", "tmp"]) };
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn shows_the_workspace_count_next_to_the_name() {
            let mut v = View { active: 1, ..view(&["cornercase", "api", "tmp"]) };
            v.projects[0].workspaces = 3;
            v.projects[2].workspaces = 0;
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn a_long_name_is_cut_before_the_count() {
            let v = View {
                projects: vec![ProjectEntry { name: "a".repeat(40), workspaces: 12, group: None, status: None }],
                ..view(&[])
            };
            let row: String =
                (0..list().width).map(|x| render(&v).backend().buffer()[(x, list().y)].symbol().to_string()).collect();
            assert!(row.contains("aaa… (12)"), "{row:?}");
        }

        #[test]
        fn truncates_long_names() {
            let v = view(&["a-folder-with-a-really-long-name-that-does-not-fit"]);
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn without_projects_the_pane_shows_the_logo() {
            insta::assert_snapshot!(render_sized(&view(&[]), W, 24).backend());
        }

        #[test]
        fn a_pane_too_short_for_the_logo_keeps_the_words() {
            let t = render_sized(&view(&[]), W, 8);
            let text: String = t.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
            assert_eq!((text.contains(TAGLINE), text.contains('█')), (true, false));
        }

        #[test]
        fn renders_active_screen_in_pane() {
            let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
            let v = View { tab: Some(single(snap)), ..view(&["~"]) };
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn places_cursor_where_the_inner_terminal_has_it() {
            let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
            let v = View { tab: Some(single(snap)), ..view(&["~"]) };
            let mut t = render(&v);
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(areas().pane.x + 2, 2));
        }
    }

    mod splits {
        use ratatui::style::Modifier;

        use super::*;
        use crate::split::Dir;

        fn three(dim_inactive: bool) -> TabView {
            let mut layout = Node::Leaf(0);
            layout.split(0, Dir::Right, 1);
            layout.split(1, Dir::Down, 2);
            let screens = vec![screen(b"$ left"), screen(b"$ top"), screen(b"$ bottom")];
            TabView { layout, screens, active: 1, dim_inactive, dragging: None, link: None, landing: None }
        }

        fn style_at(v: &View, at: Position) -> Style {
            render(v).backend().buffer()[(at.x, at.y)].style()
        }

        fn first_cell(v: &View, i: usize) -> Position {
            let tab = v.tab.as_ref().expect("a tab");
            tab.layout.panes(areas().pane)[i].1.as_position()
        }

        #[test]
        fn panes_are_drawn_with_dividers_that_join() {
            let v = View { tab: Some(three(true)), ..view(&["~"]) };
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn without_room_for_every_pane_only_the_active_one_is_drawn() {
            let v = View { tab: Some(three(true)), ..view(&["~"]) };
            insta::assert_snapshot!(render_sized(&v, W, 6).backend());
        }

        fn landing(place: Place) -> String {
            let target = three(true).layout.panes(areas().pane)[0].1;
            let tab = TabView { landing: Some((place.area(target), place)), ..three(true) };
            let v = View { tab: Some(tab), ..view(&["~"]) };
            format!("{:?}", render(&v).backend())
        }

        #[test]
        fn a_pane_landing_on_an_edge_tints_that_half() {
            insta::assert_snapshot!(landing(Place::Below));
        }

        #[test]
        fn a_pane_landing_in_the_middle_offers_a_swap() {
            insta::assert_snapshot!(landing(Place::Swap));
        }

        #[test]
        fn the_landing_takes_the_surface_background() {
            let v = View {
                tab: Some(TabView { landing: Some((areas().pane, Place::Swap)), ..three(true) }),
                ..view(&["~"])
            };
            assert_eq!(style_at(&v, areas().pane.as_position()).bg, Some(v.surface()));
        }

        #[test]
        fn inactive_panes_are_dimmed() {
            let v = View { tab: Some(three(true)), ..view(&["~"]) };
            let at = first_cell(&v, 0);
            assert!(style_at(&v, at).add_modifier.contains(Modifier::DIM));
        }

        #[test]
        fn the_active_pane_is_not_dimmed() {
            let v = View { tab: Some(three(true)), ..view(&["~"]) };
            let at = first_cell(&v, 1);
            assert!(!style_at(&v, at).add_modifier.contains(Modifier::DIM));
        }

        #[test]
        fn dimming_can_be_turned_off() {
            let v = View { tab: Some(three(false)), ..view(&["~"]) };
            let at = first_cell(&v, 0);
            assert!(!style_at(&v, at).add_modifier.contains(Modifier::DIM));
        }

        #[test]
        fn the_cursor_is_in_the_active_pane() {
            let v = View { tab: Some(three(true)), ..view(&["~"]) };
            let top = first_cell(&v, 1);
            assert_eq!(render(&v).get_cursor_position().expect("cursor"), Position::new(top.x + 5, top.y));
        }

        #[test]
        fn a_hovered_divider_turns_cyan() {
            let tab = three(true);
            let line = tab.layout.dividers(areas().pane)[0].line;
            let at = Position::new(line.x, line.y + 1);
            let v = View { tab: Some(tab), hover: Some(at), ..view(&["~"]) };
            assert_eq!(style_at(&v, at).fg, Some(Color::Cyan));
        }
    }

    mod toast {
        use super::*;

        fn copied() -> Toast<'static> {
            Toast { message: "copied to clipboard", icon: ToastIcon::Check, undo: false }
        }

        #[test]
        fn an_undo_button_sits_at_its_end() {
            let toast = Toast { message: "deleted", icon: ToastIcon::Check, undo: true };
            let (r, undo) = (toast_area(AREA, toast), toast_undo(AREA, toast));
            let t = render(&View { toast: Some(toast), ..view(&["~"]) });
            let text: String = (undo.x..undo.right()).map(|x| t.backend().buffer()[(x, undo.y)].symbol()).collect();
            assert_eq!((text.as_str(), undo.right(), undo.y), (" undo ", r.right() - 2, r.y + 1));
        }

        #[test]
        fn without_undo_there_is_no_button() {
            assert_eq!(toast_undo(AREA, copied()), Rect::default());
        }

        #[test]
        fn sits_in_the_bottom_right_corner() {
            let r = toast_area(AREA, copied());
            assert_eq!((r.right(), r.bottom(), r.height), (W - 1, H - 1, 3));
        }

        #[test]
        fn fits_a_small_screen() {
            let small = Rect::new(0, 0, 10, 2);
            assert_eq!(toast_area(small, copied()), small);
        }

        #[test]
        fn renders_over_the_pane() {
            let snap = screen(b"$ echo hello\r\nhello\r\n$ ");
            let v = View { tab: Some(single(snap)), toast: Some(copied()), ..view(&["~"]) };
            insta::assert_snapshot!(render(&v).backend());
        }

        #[rstest]
        #[case::agent_waiting(ToastIcon::Agent(Status::Waiting), "!", WAITING_COLOR)]
        #[case::agent_done(ToastIcon::Agent(Status::Done), "✓", Color::Green)]
        #[case::bug(ToastIcon::Bug, "✗", Color::Red)]
        fn its_icon_and_border_say_what_it_is_about(
            #[case] icon: ToastIcon,
            #[case] glyph: &str,
            #[case] colour: Color,
        ) {
            let toast = Toast { message: "something happened", icon, undo: false };
            let r = toast_area(AREA, toast);
            let v = View { toast: Some(toast), ..view(&["~"]) };
            let t = render(&v);
            let (icon, border) = (&t.backend().buffer()[(r.x + 2, r.y + 1)], &t.backend().buffer()[(r.x, r.y)]);
            assert_eq!((icon.symbol(), icon.fg, border.fg), (glyph, colour, colour));
        }
    }

    mod overlay {
        use super::*;

        fn form(note: Option<Note>) -> Form {
            Form {
                title: "New worktree",
                label: "branch",
                value: "feat/login".into(),
                hint: "in ~/.cornercase/worktrees/cornercase/feat-login".into(),
                toggle: None,
                note,
                submit: "create",
            }
        }

        fn with(overlay: Overlay) -> View<'static> {
            View { overlay: Some(overlay), ..view(&["cornercase"]) }
        }

        #[test]
        fn renders_the_menu_where_it_was_opened() {
            let v = with(Overlay::Menu { at: Position::new(4, 4), items: vec!["new worktree".into()] });
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn menu_stays_inside_the_screen() {
            let menu = menu_area(AREA, Position::new(W - 1, H - 1), &["new worktree"]);
            assert_eq!((menu.right(), menu.bottom()), (W, H));
        }

        #[test]
        fn menu_item_is_highlighted_on_hover() {
            let at = Position::new(4, 4);
            let item = menu_item(menu_area(AREA, at, &["new worktree"]), 0).as_position();
            let v = View { hover: Some(item), ..with(Overlay::Menu { at, items: vec!["new worktree".into()] }) };
            assert_eq!(render(&v).backend().buffer()[item].bg, DARK_SURFACE);
        }

        #[test]
        fn a_menu_leaves_what_is_behind_it_bright() {
            let at = Position::new(4, 4);
            let t = render(&with(Overlay::Menu { at, items: vec!["new worktree".into()] }));
            assert!(!t.backend().buffer()[areas().title.as_position()].modifier.contains(Modifier::DIM));
        }

        fn keys_menu() -> keys::Keys {
            keys::Keys {
                title: "Keys".into(),
                items: crate::shortcuts::items(None).iter().map(keys::Item::from).collect(),
                hint: "esc closes · ctrl+] twice types it in the pane".into(),
            }
        }

        fn keys_entry(label: &str) -> Position {
            let menu = keys_menu();
            let i = menu.items.iter().position(|item| item.label == label).expect("the entry");
            keys::item(keys::area(keys::frame(areas().pane, AREA, &menu), &menu), &menu, i).as_position()
        }

        #[test]
        fn renders_the_keys_menu_at_the_bottom_of_the_pane() {
            insta::assert_snapshot!(render(&with(Overlay::Keys(keys_menu()))).backend());
        }

        #[test]
        fn a_keys_entry_is_highlighted_on_hover() {
            let entry = keys_entry("new tab");
            let v = View { hover: Some(entry), ..with(Overlay::Keys(keys_menu())) };
            assert_eq!(render(&v).backend().buffer()[entry].bg, DARK_SURFACE);
        }

        #[test]
        fn a_keys_entry_that_names_several_keys_is_not_highlighted() {
            let entry = keys_entry("go to tab");
            let v = View { hover: Some(entry), ..with(Overlay::Keys(keys_menu())) };
            assert_eq!(render(&v).backend().buffer()[entry].bg, Color::Reset);
        }

        #[test]
        fn the_keys_menu_leaves_what_is_behind_it_bright() {
            let t = render(&with(Overlay::Keys(keys_menu())));
            assert!(!t.backend().buffer()[areas().title.as_position()].modifier.contains(Modifier::DIM));
        }

        #[test]
        fn buttons_behind_the_menu_are_not_highlighted() {
            let at = Position::new(4, list().y);
            let new = new_project_button(list(), 1, &plain(1));
            let over_new = Position::new(menu_area(AREA, at, &["new worktree"]).x + 2, new.y);
            let v = View { hover: Some(over_new), ..with(Overlay::Menu { at, items: vec!["new worktree".into()] }) };
            assert_eq!(render(&v).backend().buffer()[new.as_position()].bg, Color::Reset);
        }

        #[test]
        fn sidebar_buttons_are_not_highlighted_behind_a_form() {
            let quit = areas().quit.as_position();
            let v = View { hover: Some(quit), ..with(Overlay::Form(form(None))) };
            assert_eq!(render(&v).backend().buffer()[quit].bg, Color::Reset);
        }

        #[test]
        fn renders_a_form() {
            insta::assert_snapshot!(render(&with(Overlay::Form(form(None)))).backend());
        }

        #[test]
        fn renders_a_form_error() {
            let v = with(Overlay::Form(form(Some(Note::Error("a branch named 'feat/login' already exists".into())))));
            insta::assert_snapshot!(render(&v).backend());
        }

        #[test]
        fn puts_the_cursor_after_the_input() {
            let mut t = render(&with(Overlay::Form(form(None))));
            let input = form_rows(form_area(AREA))[1];
            let expected = input.x + u16::try_from(INPUT_PROMPT.chars().count() + "feat/login".len()).expect("width");
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(expected, input.y));
        }

        #[test]
        fn hides_the_pane_cursor_behind_an_overlay() {
            let snap = screen(b"$ ");
            let v = View { tab: Some(single(snap)), ..with(Overlay::Form(form(Some(Note::Busy("creating…"))))) };
            assert!(!render(&v).backend().cursor_visible());
        }

        #[rstest]
        #[case::submit(0, Some(FormHit::Submit))]
        #[case::cancel(1, Some(FormHit::Cancel))]
        fn buttons_are_hit(#[case] which: usize, #[case] expected: Option<FormHit>) {
            let pos = form_buttons(form_area(AREA), "create")[which].as_position();
            assert_eq!(form_hit(AREA, "create", pos), expected);
        }

        #[test]
        fn the_rest_of_the_form_is_not_a_button() {
            assert_eq!(form_hit(AREA, "create", form_area(AREA).as_position()), None);
        }
    }

    mod picker {
        use super::*;

        fn entry(name: &str, branch: Option<&str>) -> Entry {
            Entry { name: name.into(), branch: branch.map(str::to_string) }
        }

        fn picker(selected: Option<usize>) -> Picker {
            Picker {
                title: "New workspace",
                path: "~/projects/".into(),
                filter: String::new(),
                items: vec![entry("..", None), entry("cornercase", Some("main")), entry("notes", None)],
                selected,
                scroll: 0,
                hint: "enter opens ~/projects".into(),
                error: None,
                submit: "open",
                empty: "no folders here",
            }
        }

        fn with(picker: Picker) -> View<'static> {
            View { overlay: Some(Overlay::Picker(picker)), ..view(&["cornercase"]) }
        }

        fn item(i: usize) -> Position {
            picker_item(picker_area(AREA), 3, 0, i).as_position()
        }

        #[test]
        fn renders_the_folders_with_their_branch() {
            insta::assert_snapshot!(render(&with(picker(None))).backend());
        }

        #[test]
        fn says_when_nothing_matches() {
            let p = Picker { filter: "zzz".into(), items: Vec::new(), hint: String::new(), ..picker(None) };
            insta::assert_snapshot!(render(&with(p)).backend());
        }

        #[test]
        fn highlights_the_selected_folder() {
            assert_eq!(render(&with(picker(Some(1)))).backend().buffer()[item(1)].bg, DARK_SURFACE);
        }

        #[test]
        fn highlights_the_hovered_folder() {
            let v = View { hover: Some(item(2)), ..with(picker(None)) };
            assert_eq!(render(&v).backend().buffer()[item(2)].bg, DARK_HOVER);
        }

        #[test]
        fn puts_the_cursor_after_the_path() {
            let mut t = render(&with(picker(None)));
            let input = picker_rows(picker_area(AREA))[0];
            let expected = input.x + u16::try_from(INPUT_PROMPT.chars().count() + "~/projects/".len()).expect("width");
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(expected, input.y));
        }

        #[rstest]
        #[case::folder(item(1), Some(PickerHit::Item(1)))]
        #[case::below_the_last_folder(item(2).offset(ratatui::layout::Offset { x: 0, y: 1 }), None)]
        #[case::submit(picker_buttons(picker_area(AREA), "open")[0].as_position(), Some(PickerHit::Submit))]
        #[case::cancel(picker_buttons(picker_area(AREA), "open")[1].as_position(), Some(PickerHit::Cancel))]
        #[case::the_input(picker_rows(picker_area(AREA))[0].as_position(), None)]
        fn maps_clicks(#[case] pos: Position, #[case] expected: Option<PickerHit>) {
            assert_eq!(picker_hit(AREA, "open", 3, 0, pos), expected);
        }

        #[test]
        fn scrolled_rows_map_to_later_folders() {
            let list = picker_list(picker_area(AREA));
            let items = usize::from(list.height) + 5;
            assert_eq!(picker_hit(AREA, "open", items, 2, list.as_position()), Some(PickerHit::Item(2)));
        }

        #[test]
        fn scroll_never_leaves_empty_rows_at_the_bottom() {
            let list = picker_list(picker_area(AREA));
            let items = usize::from(list.height) + 5;
            assert_eq!(picker_hit(AREA, "open", items, 100, list.as_position()), Some(PickerHit::Item(5)));
        }

        #[test]
        fn folders_scrolled_out_have_no_row() {
            assert!(picker_item(picker_area(AREA), 50, 10, 3).is_empty());
        }
    }

    mod settings {
        use super::*;

        fn row(section: &'static str, label: &str, value: &str, note: &str) -> SettingsRow {
            SettingsRow {
                section,
                label: label.into(),
                value: value.into(),
                note: note.into(),
                dangerous: false,
                removable: false,
                movable: false,
            }
        }

        const TABS: [&str; 4] = ["Worktrees", "Agents", "Issues", "TUI"];

        fn source(label: &str, note: &str) -> SettingsRow {
            SettingsRow { movable: true, ..row("Sources shown", label, "", note) }
        }

        fn settings(cursor: usize) -> Settings {
            Settings {
                tabs: TABS.to_vec(),
                tab: 2,
                rows: vec![
                    SettingsRow { removable: true, ..row("Accounts", "Shortcut API token", "@ana in acme", "saved") },
                    row("Accounts", "Linear API key", "not connected", "enter pastes one"),
                    source("[x] All", "every source together"),
                    source("[x] GitHub", ""),
                    source("[ ] Linear", ""),
                ],
                cursor,
                edit: None,
                pick: None,
                note: None,
                hint: "enter changes the selected setting".into(),
                submit: "done",
            }
        }

        fn with(settings: Settings) -> View<'static> {
            View { overlay: Some(Overlay::Settings(settings)), ..view(&["shop"]) }
        }

        fn sections() -> Vec<&'static str> {
            settings(0).sections()
        }

        fn layout<'a>(sections: &'a [&'a str]) -> SettingsLayout<'a> {
            SettingsLayout {
                sections,
                tabs: &TABS,
                removable: &[true, false, false, false, false],
                movable: &[false, false, true, true, true],
                cursor: 0,
                pick: None,
            }
        }

        #[test]
        fn renders_the_sections_and_rows() {
            insta::assert_snapshot!(render(&with(settings(1))).backend());
        }

        #[test]
        fn renders_a_pick() {
            let pick = SettingsPick {
                title: "How should claude start?".into(),
                filter: "pl".into(),
                items: vec![
                    ("plan".into(), "--permission-mode plan".into(), false),
                    ("skip permissions (dangerous)".into(), "--dangerously-skip-permissions".into(), true),
                ],
                selected: Some(0),
                scroll: 0,
            };
            let agents = Settings { tab: 1, rows: vec![row("Agent", "default agent", "claude", "")], ..settings(0) };
            insta::assert_snapshot!(render(&with(Settings { pick: Some(pick), ..agents })).backend());
        }

        #[test]
        fn headers_go_before_the_first_row_of_each_section() {
            assert_eq!(
                settings_lines(&["Folder", "Accounts", "Accounts"]),
                [
                    SettingsLine::Header(0),
                    SettingsLine::Row(0),
                    SettingsLine::Blank,
                    SettingsLine::Header(1),
                    SettingsLine::Row(1),
                    SettingsLine::Row(2)
                ]
            );
        }

        #[test]
        fn a_row_without_a_section_has_no_header() {
            assert_eq!(
                settings_lines(&["", "Agent"]),
                [SettingsLine::Row(0), SettingsLine::Blank, SettingsLine::Header(1), SettingsLine::Row(1)]
            );
        }

        #[test]
        fn the_active_tab_is_filled() {
            let t = render(&with(settings(0)));
            let issues = settings_tabs(settings_area(AREA), &TABS)[2];
            assert_eq!(t.backend().buffer()[issues.as_position()].bg, DARK_SURFACE);
        }

        #[test]
        fn scrolls_to_keep_the_selected_row_visible() {
            let sections = vec!["A"; 40];
            let at = settings_scroll(&sections, 30, 10);
            let line = settings_lines(&sections).iter().position(|l| *l == SettingsLine::Row(30)).expect("row");
            assert!(line >= at && line < at + 10, "line {line} scroll {at}");
        }

        #[test]
        fn a_dangerous_value_is_red() {
            let claude = SettingsRow {
                dangerous: true,
                ..row("How each agent starts", "claude", "skip permissions (dangerous)", "the default agent")
            };
            let r = settings_row(AREA, &["How each agent starts"], 0, 0);
            let t = render(&with(Settings { tab: 1, rows: vec![claude], ..settings(0) }));
            let cell =
                (r.x..r.right()).map(|x| &t.backend().buffer()[(x, r.y)]).find(|c| c.symbol() == "k").expect("text");
            assert_eq!(cell.fg, Color::Red);
        }

        #[test]
        fn the_edit_line_puts_the_cursor_after_the_value() {
            let edit = SettingsEdit { label: "Linear API key".into(), value: "•••".into() };
            let mut t = render(&with(Settings { edit: Some(edit), ..settings(2) }));
            let row = settings_edit(settings_area(AREA));
            let x = row.x
                + u16::try_from(INPUT_PROMPT.chars().count() + "Linear API key: ".chars().count() + 3).expect("width");
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(x, row.y));
        }

        #[rstest]
        #[case::a_tab(settings_tabs(settings_area(AREA), &TABS)[1].as_position(), Some(SettingsHit::Tab(1)))]
        #[case::a_row(settings_row(AREA, &sections(), 0, 1).as_position(), Some(SettingsHit::Row(1)))]
        #[case::remove(settings_remove(settings_row(AREA, &sections(), 0, 0)).as_position(), Some(SettingsHit::Remove(0)))]
        #[case::move_up(settings_moves(settings_row(AREA, &sections(), 0, 3))[0].as_position(), Some(SettingsHit::MoveUp(3)))]
        #[case::move_down(settings_moves(settings_row(AREA, &sections(), 0, 3))[1].as_position(), Some(SettingsHit::MoveDown(3)))]
        #[case::done(settings_done(settings_area(AREA)).as_position(), Some(SettingsHit::Done))]
        #[case::a_header(settings_body(settings_area(AREA)).as_position(), None)]
        fn maps_clicks(#[case] pos: Position, #[case] expected: Option<SettingsHit>) {
            let sections = sections();
            assert_eq!(settings_hit(AREA, &layout(&sections), pos), expected);
        }

        #[test]
        fn a_click_in_a_pick_chooses_that_item() {
            let sections = sections();
            let list = settings_pick_list(settings_area(AREA));
            let pos = Position::new(list.x + 2, list.y + 1);
            let pick = SettingsLayout { pick: Some((3, 0)), ..layout(&sections) };
            assert_eq!(settings_hit(AREA, &pick, pos), Some(SettingsHit::Pick(1)));
        }
    }

    mod issues {
        use super::*;

        const TABS: [&str; 4] = ["All", "GitHub", "Shortcut", "Linear"];
        const TOGGLES: [&str; 2] = ["closed", "mine"];

        fn row(key: &str, title: &str, meta: &str) -> IssueRow {
            IssueRow { key: key.into(), title: title.into(), meta: meta.into() }
        }

        fn issues(body: IssuesBody) -> Issues {
            Issues {
                title: "Issues · shop".into(),
                tabs: TABS.to_vec(),
                tab: 0,
                toggles: TOGGLES.map(String::from).to_vec(),
                on: vec![false, true],
                body,
                note: None,
                hint: "enter reads #482 · start works on it in its own worktree, on issue-482-returns".into(),
                buttons: vec!["start", "cancel"],
            }
        }

        fn list(selected: Option<usize>) -> IssuesBody {
            IssuesBody::List {
                filter: String::new(),
                items: vec![
                    row("#482", "Returns page crashes on empty address", "bug · ana · 3d"),
                    row("sc-48", "Dark mode", "In Review · luis · 2mo"),
                ],
                selected,
                scroll: 0,
                empty: "no open issues".into(),
            }
        }

        fn with(issues: Issues) -> View<'static> {
            View { overlay: Some(Overlay::Issues(issues)), ..view(&["shop"]) }
        }

        fn item(i: usize) -> Position {
            list_item(issues_list(issues_area(AREA)), 2, 0, i).as_position()
        }

        fn text(v: &View) -> String {
            render(v).backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect()
        }

        #[test]
        fn renders_the_tabs_toggles_and_issues() {
            insta::assert_snapshot!(render(&with(issues(list(Some(0))))).backend());
        }

        #[test]
        fn renders_the_token_form() {
            let body = IssuesBody::Token {
                label: "API token",
                input: "•••".into(),
                help: vec!["Connect Shortcut.".into(), String::new(), "Paste a token.".into()],
            };
            let i = Issues { tab: 2, buttons: vec!["connect", "cancel"], ..issues(body) };
            insta::assert_snapshot!(render(&with(i)).backend());
        }

        #[test]
        fn renders_an_issue_from_its_scroll() {
            let lines = (0..40).map(|n| Line::from(format!("line {n}"))).collect();
            let i = Issues { buttons: vec!["start", "back"], ..issues(IssuesBody::Detail { lines, scroll: 5 }) };
            let first = issue_detail(issues_area(AREA));
            let t = render(&with(i));
            let row: String =
                (first.x..first.right()).map(|x| t.backend().buffer()[(x, first.y)].symbol().to_string()).collect();
            assert!(row.starts_with("line 5"), "{row:?}");
        }

        #[test]
        fn the_active_tab_is_filled() {
            let i = issues(list(None));
            let tab = issue_tabs(issues_area(AREA), &TABS)[0].as_position();
            assert_eq!(render(&with(i)).backend().buffer()[tab].bg, DARK_SURFACE);
        }

        #[test]
        fn a_toggle_shows_whether_it_is_on() {
            let t = text(&with(issues(list(None))));
            assert!(t.contains("[ ] closed") && t.contains("[x] mine"), "{t}");
        }

        #[test]
        fn highlights_the_selected_issue() {
            assert_eq!(render(&with(issues(list(Some(1))))).backend().buffer()[item(1)].bg, DARK_SURFACE);
        }

        #[test]
        fn highlights_the_hovered_issue() {
            let v = View { hover: Some(item(1)), ..with(issues(list(None))) };
            assert_eq!(render(&v).backend().buffer()[item(1)].bg, DARK_HOVER);
        }

        #[test]
        fn says_why_the_list_is_empty() {
            let body = IssuesBody::List {
                filter: "zzz".into(),
                items: Vec::new(),
                selected: None,
                scroll: 0,
                empty: "no matches".into(),
            };
            assert!(text(&with(issues(body))).contains("no matches"));
        }

        #[test]
        fn an_error_replaces_the_hint() {
            let i = Issues { note: Some(Note::Error("no git remotes found".into())), ..issues(list(None)) };
            let t = text(&with(i));
            assert!(t.contains("no git remotes found") && !t.contains("enter reads"), "{t}");
        }

        #[rstest]
        #[case::tab(issue_tabs(issues_area(AREA), &TABS)[3].as_position(), Some(IssuesHit::Tab(3)))]
        #[case::toggle(issue_toggles(issues_area(AREA), &TOGGLES)[1].as_position(), Some(IssuesHit::Toggle(1)))]
        #[case::issue(item(1), Some(IssuesHit::Item(1)))]
        #[case::below_the_last(item(1).offset(ratatui::layout::Offset { x: 0, y: 1 }), None)]
        #[case::start(issue_buttons(issues_area(AREA), &["start", "cancel"])[0].as_position(), Some(IssuesHit::Button(0)))]
        #[case::cancel(issue_buttons(issues_area(AREA), &["start", "cancel"])[1].as_position(), Some(IssuesHit::Button(1)))]
        fn maps_clicks(#[case] pos: Position, #[case] expected: Option<IssuesHit>) {
            assert_eq!(issues_hit(AREA, &TABS, &TOGGLES, &["start", "cancel"], 2, 0, pos), expected);
        }

        #[test]
        fn buttons_end_at_the_right_edge_in_order() {
            let b = issue_buttons(issues_area(AREA), &["start", "disconnect", "cancel"]);
            assert!(
                b[0].right() < b[1].x
                    && b[1].right() < b[2].x
                    && b[2].right() == issues_rows(issues_area(AREA))[4].right()
            );
        }

        #[test]
        fn the_button_shows_only_with_a_project() {
            let shown = |v: &View| text(v).contains(ISSUES_LABEL);
            let project = View { has_project: true, issues: true, ..view(&["shop"]) };
            assert_eq!((shown(&view(&[])), shown(&project)), (false, true));
        }

        #[test]
        fn the_button_sits_level_with_settings() {
            assert_eq!(areas().issues.y, areas().settings.y);
        }

        #[test]
        fn the_button_is_filled_on_hover() {
            let pos = areas().issues.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 });
            let v = View { has_project: true, issues: true, hover: Some(pos), ..view(&["shop"]) };
            assert_eq!(render(&v).backend().buffer()[pos].bg, Color::Cyan);
        }
    }

    mod search {
        use super::*;

        fn results() -> Rect {
            areas().results
        }

        fn search(query: &str, results: Vec<ResultRow>) -> Search {
            Search { query: query.into(), results, selected: 0, scroll: 0, hint: "enter goes to feat/search".into() }
        }

        fn row(name: &str, context: &str) -> ResultRow {
            ResultRow { name: name.into(), context: context.into() }
        }

        fn found() -> Vec<ResultRow> {
            vec![row("feat/search", "notes"), row("nvim", "notes › feat/search")]
        }

        fn with(search: Search) -> View<'static> {
            View { overlay: Some(Overlay::Search(search)), ..view(&["cornercase", "notes"]) }
        }

        fn item(i: usize) -> Position {
            result_item(results(), 2, 0, i).as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
        }

        #[test]
        fn the_bar_shows_a_placeholder_when_closed() {
            let bar = areas().search;
            let text: String = (bar.x..bar.right())
                .map(|x| render(&view(&[])).backend().buffer()[(x, bar.y)].symbol().to_string())
                .collect();
            assert!(text.contains(SEARCH_PLACEHOLDER), "{text:?}");
        }

        #[test]
        fn renders_the_results_over_both_columns() {
            insta::assert_snapshot!(render(&with(search("feat", found()))).backend());
        }

        #[test]
        fn an_empty_query_keeps_the_columns() {
            insta::assert_snapshot!(render(&with(search("", Vec::new()))).backend());
        }

        #[test]
        fn says_when_nothing_matches() {
            let text: String = render(&with(search("zzz", Vec::new())))
                .backend()
                .buffer()
                .content()
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect();
            assert!(text.contains("no matches"), "{text}");
        }

        #[test]
        fn highlights_the_selected_result() {
            assert_eq!(render(&with(search("feat", found()))).backend().buffer()[item(0)].bg, DARK_SURFACE);
        }

        #[test]
        fn highlights_the_hovered_result() {
            let v = View { hover: Some(item(1)), ..with(search("feat", found())) };
            assert_eq!(render(&v).backend().buffer()[item(1)].bg, DARK_HOVER);
        }

        #[test]
        fn marks_the_matching_letters() {
            let s = Search { selected: 1, ..search("feat", found()) };
            assert_eq!(render(&with(s)).backend().buffer()[item(0)].fg, Color::Cyan);
        }

        #[test]
        fn puts_the_cursor_after_the_query() {
            let mut t = render(&with(search("feat", found())));
            let bar = areas().search;
            let x = bar.x + u16::try_from(SEARCH_ICON.chars().count() + "feat".len()).expect("width");
            assert_eq!(t.get_cursor_position().expect("cursor"), Position::new(x, bar.y));
        }

        #[rstest]
        #[case::first(0, Some(0))]
        #[case::second(1, Some(1))]
        #[case::below_the_last(2, None)]
        fn maps_clicks(#[case] row: u16, #[case] expected: Option<usize>) {
            let list = results_list(results());
            assert_eq!(result_hit(results(), 2, 0, Position::new(list.x + 3, list.y + row)), expected);
        }

        #[test]
        fn scrolled_rows_map_to_later_results() {
            let list = results_list(results());
            let items = usize::from(list.height) + 5;
            assert_eq!(result_hit(results(), items, 3, list.as_position()), Some(3));
        }
    }

    mod display_path {
        use super::*;

        #[rstest]
        #[case::under_home("/home/ana/.cornercase/w", "~/.cornercase/w")]
        #[case::home_itself("/home/ana", "~")]
        #[case::elsewhere("/srv/w", "/srv/w")]
        fn shows_home_as_tilde(#[case] path: &str, #[case] expected: &str) {
            assert_eq!(display_path(Path::new(path), Some(Path::new("/home/ana"))), expected);
        }
    }

    mod hover {
        use super::*;

        fn close_pos() -> Position {
            Position::new(close_x(), list().y)
        }

        fn new_pos() -> Position {
            Position::new(3, new_project_button(list(), 1, &plain(1)).y)
        }

        #[test]
        fn close_button_is_hidden_by_default() {
            assert_eq!(render(&view(&["~"])).backend().buffer()[close_pos()].symbol(), " ");
        }

        #[test]
        fn close_button_shows_dim_while_the_row_is_hovered() {
            let v = View { hover: Some(Position::new(list().x + 3, list().y)), ..view(&["~"]) };
            let cell = render(&v).backend().buffer()[close_pos()].clone();
            assert_eq!((cell.symbol(), cell.fg), ("×", Color::DarkGray));
        }

        #[test]
        fn the_active_entry_has_a_background() {
            let pos = Position::new(list().x + 3, list().y);
            assert_eq!(render(&view(&["~", "tmp"])).backend().buffer()[pos].bg, DARK_SURFACE);
        }

        #[test]
        fn the_surface_is_light_on_a_light_theme() {
            let pos = Position::new(list().x + 3, list().y);
            assert_eq!(render(&View { light: true, ..view(&["~"]) }).backend().buffer()[pos].bg, LIGHT_SURFACE);
        }

        #[test]
        fn the_menu_button_shows_beside_the_close_button_while_the_row_is_hovered() {
            let menu = row_menu_button(entry_row(list(), 1, &plain(1), 0, SidebarRow::Project(0)), 1);
            let at = Position::new(menu.right() - 1, menu.y);
            let hidden = render(&view(&["~"])).backend().buffer()[at].symbol().to_string();
            let v = View { hover: Some(Position::new(list().x + 3, list().y)), ..view(&["~"]) };
            let cell = render(&v).backend().buffer()[at].clone();
            assert_eq!((hidden.as_str(), cell.symbol(), cell.fg), (" ", ROW_MENU_ICON, Color::DarkGray));
        }

        #[test]
        fn the_menu_button_turns_cyan_on_hover() {
            let menu = row_menu_button(entry_row(list(), 1, &plain(1), 0, SidebarRow::Project(0)), 1);
            let v = View { hover: Some(menu.as_position()), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[Position::new(menu.right() - 1, menu.y)].fg, Color::Cyan);
        }

        #[test]
        fn close_button_turns_red_on_hover() {
            let v = View { hover: Some(close_pos()), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[close_pos()].fg, Color::Red);
        }

        #[test]
        fn new_button_has_no_background_by_default() {
            assert_eq!(render(&view(&["~"])).backend().buffer()[new_pos()].bg, Color::Reset);
        }

        fn quit_pos() -> Position {
            areas().quit.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
        }

        #[test]
        fn quit_button_is_dim_by_default() {
            assert_eq!(render(&view(&["~"])).backend().buffer()[quit_pos()].bg, Color::Reset);
        }

        fn settings_pos() -> Position {
            areas().settings.as_position().offset(ratatui::layout::Offset { x: 2, y: 0 })
        }

        #[test]
        fn settings_button_is_dim_by_default() {
            assert_eq!(render(&view(&["~"])).backend().buffer()[settings_pos()].bg, Color::Reset);
        }

        #[test]
        fn settings_button_is_filled_on_hover() {
            let v = View { hover: Some(settings_pos()), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[settings_pos()].bg, Color::Cyan);
        }

        #[test]
        fn quit_button_turns_red_on_hover() {
            let v = View { hover: Some(quit_pos()), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[quit_pos()].bg, Color::Red);
        }

        #[test]
        fn new_button_is_filled_on_hover() {
            let v = View { hover: Some(new_pos()), ..view(&["~"]) };
            assert_eq!(render(&v).backend().buffer()[new_pos()].bg, Color::Cyan);
        }
    }

    mod row_hover {
        use super::*;

        #[derive(Debug, Clone, Copy)]
        enum Row {
            Project(usize),
            Group,
            Workspace,
            Tab(usize),
        }

        impl Row {
            fn nav(self) -> Nav {
                match self {
                    Row::Project(_) | Row::Group => Nav::Projects,
                    Row::Workspace | Row::Tab(_) => Nav::Workspaces,
                }
            }

            fn active(self) -> bool {
                matches!(self, Row::Project(0) | Row::Tab(0))
            }
        }

        const COMPACT: Rect = Rect { x: 0, y: 0, width: 80, height: 30 };

        fn sample(light: bool) -> View<'static> {
            let mut v = View {
                has_project: true,
                workspaces: vec![WorkspaceEntry {
                    name: "login".into(),
                    tabs: vec!["claude".into(), "nvim".into()],
                    behind: 3,
                    removing: false,
                }],
                active_tab: Some(0),
                light,
                ..view(&["cornercase", "shop"])
            };
            v.groups = vec![GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false }];
            v.projects[1].group = Some(0);
            v
        }

        fn rect(v: &View, size: Rect, row: Row) -> Rect {
            let a = layout(size, v.widths).shown(v.nav);
            let sidebar = |r| entry_row(a.list, a.pitch, &v.sidebar_rows(), v.projects_scroll, r);
            let workspaces = |r| workspace_row(a.workspaces_list, a.pitch, &v.tab_lines(), v.workspaces_scroll, r);
            match row {
                Row::Project(p) => sidebar(SidebarRow::Project(p)),
                Row::Group => sidebar(SidebarRow::Group(0)),
                Row::Workspace => workspaces(WorkspaceRow::Workspace(0)),
                Row::Tab(t) => workspaces(WorkspaceRow::Tab(0, t)),
            }
        }

        fn backgrounds(v: &View, size: Rect, r: Rect) -> Vec<Color> {
            let t = render_sized(v, size.width, size.height);
            let mut colours: Vec<Color> = r.positions().map(|p| t.backend().buffer()[p].bg).collect::<Vec<_>>();
            colours.dedup();
            colours
        }

        fn hovering(mut v: View<'static>, size: Rect, row: Row) -> (View<'static>, Rect) {
            let r = rect(&v, size, row);
            v.hover = Some(Position::new(r.x + 4, middle(r).y));
            (v, r)
        }

        #[rstest]
        fn a_hovered_row_is_filled_edge_to_edge(
            #[values(Row::Project(0), Row::Project(1), Row::Group, Row::Workspace, Row::Tab(0), Row::Tab(1))] row: Row,
            #[values(false, true)] light: bool,
            #[values(false, true)] compact: bool,
        ) {
            let (size, nav) = if compact { (COMPACT, Some(row.nav())) } else { (AREA, None) };
            let (v, r) = hovering(View { nav, ..sample(light) }, size, row);
            let expected = match (row.active(), light) {
                (true, false) => DARK_SURFACE,
                (true, true) => LIGHT_SURFACE,
                (false, false) => DARK_HOVER,
                (false, true) => LIGHT_HOVER,
            };
            assert_eq!(backgrounds(&v, size, r), vec![expected]);
        }

        #[test]
        fn the_close_button_and_the_behind_tag_sit_on_the_hover_background() {
            let (v, r) = hovering(sample(false), AREA, Row::Workspace);
            let t = render(&v);
            let cells: Vec<(String, Color)> = [r.right() - 2, r.right() - 7]
                .into_iter()
                .map(|x| t.backend().buffer()[(x, r.y)].clone())
                .map(|c| (c.symbol().to_string(), c.bg))
                .collect();
            assert_eq!(cells, vec![("×".into(), DARK_HOVER), ("3".into(), DARK_HOVER)]);
        }

        #[rstest]
        fn a_hovered_group_header_shows_its_close_button(#[values(false, true)] compact: bool) {
            let (size, nav) = if compact { (COMPACT, Some(Nav::Projects)) } else { (AREA, None) };
            let (v, r) = hovering(View { nav, ..sample(false) }, size, Row::Group);
            let close = row_close_button(r, layout(size, v.widths).pitch);
            let t = render_sized(&v, size.width, size.height);
            let cell = t.backend().buffer()[(close.x + close.width / 2, middle(close).y)].clone();
            assert_eq!((cell.symbol(), cell.fg), ("×", Color::DarkGray));
        }

        #[test]
        fn a_long_group_name_is_cut_before_the_close_button() {
            let mut v = sample(false);
            v.groups[0].name = "x".repeat(200);
            let (v, r) = hovering(v, AREA, Row::Group);
            let menu = row_menu_button(r, 1);
            let t = render(&v);
            let cell = |x: u16| t.backend().buffer()[(x, r.y)].symbol().to_string();
            assert_eq!([cell(menu.x - 2), cell(menu.x - 1)], ["…", " "]);
        }

        #[test]
        fn other_rows_stay_plain() {
            let (v, _) = hovering(sample(false), AREA, Row::Tab(1));
            assert_eq!(backgrounds(&v, AREA, rect(&v, AREA, Row::Project(1))), vec![Color::Reset]);
        }

        #[test]
        fn no_row_is_lit_while_an_overlay_is_open() {
            let (mut v, r) = hovering(sample(false), AREA, Row::Project(1));
            v.overlay = Some(Overlay::Menu { at: Position::new(80, 1), items: vec!["rename tab".into()] });
            assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
        }

        #[test]
        fn no_row_is_lit_while_dragging_a_border() {
            let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
            let v = View { resizing: Some(Border::Workspaces), ..v };
            assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
        }

        #[test]
        fn no_row_is_lit_while_dragging_a_divider() {
            let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
            let tab = TabView { dragging: Some(Vec::new()), ..single(screen(b"")) };
            let v = View { tab: Some(tab), ..v };
            assert_eq!(backgrounds(&v, AREA, r), vec![Color::Reset]);
        }

        #[test]
        fn no_row_is_lit_and_no_close_button_shows_while_dragging_a_row() {
            let (v, r) = hovering(sample(false), AREA, Row::Tab(1));
            let v = View { drag: Some(Drag::Sidebar(SidebarRow::Project(1), None)), ..v };
            let t = render(&v);
            assert_eq!(
                (backgrounds(&v, AREA, r), t.backend().buffer()[(r.right() - 2, r.y)].symbol()),
                (vec![Color::Reset], " ")
            );
        }

        #[rstest]
        fn the_dragged_row_keeps_the_surface_background(
            #[values(Row::Project(1), Row::Group, Row::Workspace, Row::Tab(1))] row: Row,
            #[values(false, true)] compact: bool,
        ) {
            let (size, nav) = if compact { (COMPACT, Some(row.nav())) } else { (AREA, None) };
            let v = View { nav, ..sample(false) };
            let drag = match row {
                Row::Project(p) => Drag::Sidebar(SidebarRow::Project(p), None),
                Row::Group => Drag::Sidebar(SidebarRow::Group(0), None),
                Row::Workspace => Drag::Workspaces(WorkspaceRow::Workspace(0), None),
                Row::Tab(t) => Drag::Workspaces(WorkspaceRow::Tab(0, t), None),
            };
            let v = View { drag: Some(drag), ..v };
            assert_eq!(backgrounds(&v, size, rect(&v, size, row)), vec![DARK_SURFACE]);
        }
    }

    mod reorder {
        use super::*;

        fn grouped() -> View<'static> {
            let mut v = View { active: 1, has_project: true, ..view(&["tmp", "api", "web", "cornercase"]) };
            v.groups = vec![
                GroupEntry { name: "work".into(), icon: '●', colour: 4, collapsed: false },
                GroupEntry { name: "oss".into(), icon: '★', colour: 99, collapsed: false },
            ];
            for (p, g) in [(1, 0), (2, 0), (3, 1)] {
                v.projects[p].group = Some(g);
            }
            v
        }

        fn sidebar_at(dragged: SidebarRow, y: u16) -> Option<Landing> {
            let rows = grouped().sidebar_rows();
            sidebar_drop(list(), 1, &rows, 0, dragged, Position::new(list().x + 3, y))
        }

        fn row(i: u16) -> u16 {
            list().y + i
        }

        fn spot(group: Option<usize>, before: Option<usize>) -> Spot {
            Spot::Project { group, before }
        }

        #[test]
        fn the_rows_are_a_loose_project_then_two_groups() {
            assert_eq!(
                grouped().sidebar_rows(),
                [
                    SidebarRow::Project(0),
                    SidebarRow::Gap,
                    SidebarRow::Group(0),
                    SidebarRow::Project(1),
                    SidebarRow::Project(2),
                    SidebarRow::Gap,
                    SidebarRow::Group(1),
                    SidebarRow::Project(3),
                ]
            );
        }

        #[rstest]
        #[case::onto_itself(row(0), None)]
        #[case::onto_a_project_below_lands_after_it(row(3), Some((4, spot(Some(0), Some(2)))))]
        #[case::onto_a_header_lands_first_in_the_group(row(2), Some((3, spot(Some(0), Some(1)))))]
        #[case::onto_the_last_of_a_group_lands_at_its_end(row(4), Some((5, spot(Some(0), None))))]
        #[case::onto_the_gap_below_a_group_lands_at_its_end(row(5), Some((5, spot(Some(0), None))))]
        #[case::onto_the_last_header(row(6), Some((7, spot(Some(1), Some(3)))))]
        #[case::below_every_row(row(9), Some((8, spot(Some(1), None))))]
        #[case::outside_the_list(list().bottom(), None)]
        fn a_loose_project_dragged_down(#[case] y: u16, #[case] expected: Option<(usize, Spot)>) {
            let expected = expected.map(|(at, spot)| Landing { at, spot });
            assert_eq!(sidebar_at(SidebarRow::Project(0), y), expected);
        }

        #[rstest]
        #[case::onto_a_loose_project_above_lands_before_it(row(0), Some((0, spot(None, Some(0)))))]
        #[case::onto_the_gap_below_the_loose_ones_ungroups_it(row(1), Some((1, spot(None, None))))]
        #[case::onto_a_header(row(2), Some((3, spot(Some(0), Some(1)))))]
        #[case::onto_a_project_above_lands_before_it(row(4), Some((4, spot(Some(0), Some(2)))))]
        #[case::onto_its_own_header_stays(row(6), Some((7, spot(Some(1), None))))]
        #[case::above_the_list(list().y - 1, Some((0, spot(None, Some(0)))))]
        fn a_grouped_project_dragged_up(#[case] y: u16, #[case] expected: Option<(usize, Spot)>) {
            let expected = expected.map(|(at, spot)| Landing { at, spot });
            assert_eq!(sidebar_at(SidebarRow::Project(3), y), expected);
        }

        #[rstest]
        #[case::onto_a_loose_project_lands_first(SidebarRow::Group(1), row(0), Some((2, Spot::Group(0))))]
        #[case::onto_a_project_of_a_group_above(SidebarRow::Group(1), row(3), Some((2, Spot::Group(0))))]
        #[case::onto_the_gap_right_above_itself(SidebarRow::Group(1), row(5), Some((6, Spot::Group(1))))]
        #[case::onto_one_of_its_projects(SidebarRow::Group(1), row(7), None)]
        #[case::onto_a_group_below_lands_after_it(SidebarRow::Group(0), row(6), Some((8, Spot::Group(2))))]
        #[case::below_every_row(SidebarRow::Group(0), row(9), Some((8, Spot::Group(2))))]
        fn a_group_lands_among_groups(
            #[case] dragged: SidebarRow,
            #[case] y: u16,
            #[case] expected: Option<(usize, Spot)>,
        ) {
            let expected = expected.map(|(at, spot)| Landing { at, spot });
            assert_eq!(sidebar_at(dragged, y), expected);
        }

        fn workspaces_at(dragged: WorkspaceRow, row: WorkspaceRow) -> Option<Landing> {
            let (list, tabs) = (Rect { height: 20, ..areas().workspaces_list }, tabs(&[2, 1, 2]));
            let y = workspace_row(list, 1, &tabs, 0, row).y;
            workspace_drop(list, 1, &tabs, 0, dragged, Position::new(list.x + 3, y))
        }

        #[rstest]
        #[case::onto_a_tab_of_a_workspace_above(WorkspaceRow::Workspace(2), WorkspaceRow::Tab(0, 1), 0, 0)]
        #[case::onto_the_plus_tab_of_a_workspace_above(WorkspaceRow::Workspace(2), WorkspaceRow::NewTab(1), 5, 1)]
        #[case::onto_a_workspace_below(WorkspaceRow::Workspace(0), WorkspaceRow::Tab(1, 0), 8, 2)]
        #[case::onto_the_last_workspace(WorkspaceRow::Workspace(0), WorkspaceRow::Workspace(2), 13, 3)]
        fn a_workspace_lands_before_or_after_the_one_under_the_mouse(
            #[case] dragged: WorkspaceRow,
            #[case] under: WorkspaceRow,
            #[case] at: usize,
            #[case] before: usize,
        ) {
            assert_eq!(workspaces_at(dragged, under), Some(Landing { at, spot: Spot::Workspace(before) }));
        }

        #[rstest]
        #[case::onto_the_next_tab(WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(0, 1), 3, 2)]
        #[case::onto_plus_tab(WorkspaceRow::Tab(0, 0), WorkspaceRow::NewTab(0), 3, 2)]
        #[case::onto_its_workspace(WorkspaceRow::Tab(2, 1), WorkspaceRow::Workspace(2), 10, 0)]
        #[case::onto_a_workspace_below_stays_in_its_own(WorkspaceRow::Tab(0, 0), WorkspaceRow::Tab(1, 0), 3, 2)]
        #[case::onto_a_workspace_above_stays_in_its_own(WorkspaceRow::Tab(2, 1), WorkspaceRow::Tab(0, 0), 10, 0)]
        fn a_tab_lands_within_its_workspace(
            #[case] dragged: WorkspaceRow,
            #[case] under: WorkspaceRow,
            #[case] at: usize,
            #[case] before: usize,
        ) {
            assert_eq!(workspaces_at(dragged, under), Some(Landing { at, spot: Spot::Tab(before) }));
        }

        #[test]
        fn a_tab_dropped_on_itself_goes_nowhere() {
            assert_eq!(workspaces_at(WorkspaceRow::Tab(0, 1), WorkspaceRow::Tab(0, 1)), None);
        }

        fn many(n: usize, scroll: usize) -> Rows {
            project_rows(list(), 1, &plain(n), scroll)
        }

        #[rstest]
        #[case::the_line_above_scrolls_up(3, more_above(list()).as_position(), Some(-1))]
        #[case::the_line_below_scrolls_down(3, many(20, 3).more_below().as_position(), Some(1))]
        #[case::the_new_button_scrolls_down(3, many(20, 3).button().as_position(), Some(1))]
        #[case::a_row_does_not_scroll(3, Position::new(3, list().y + 2), None)]
        #[case::nothing_hidden_above(0, more_above(list()).as_position(), None)]
        fn the_edges_of_a_long_list_scroll_it(
            #[case] scroll: usize,
            #[case] pos: Position,
            #[case] expected: Option<isize>,
        ) {
            assert_eq!(many(20, scroll).edge(pos), expected);
        }

        #[test]
        fn a_short_list_never_scrolls() {
            assert_eq!(many(3, 0).edge(many(3, 0).more_below().as_position()), None);
        }

        fn landed_rows(rows: &[SidebarRow], scroll: usize, at: usize) -> (Vec<SidebarRow>, Rect) {
            let layout = project_rows(list(), 1, rows, scroll);
            let landing = Landing { at, spot: Spot::Group(0) };
            landed(rows.to_vec(), &layout, Some(landing), SidebarRow::Gap, SidebarRow::Landing)
        }

        #[test]
        fn the_line_goes_between_two_rows() {
            let (rows, line) = landed_rows(&plain(3), 0, 1);
            assert_eq!(
                (rows, line),
                (
                    vec![SidebarRow::Project(0), SidebarRow::Landing, SidebarRow::Project(1), SidebarRow::Project(2)],
                    Rect::default()
                )
            );
        }

        #[test]
        fn the_line_takes_the_place_of_a_gap() {
            let rows = grouped().sidebar_rows();
            let (drawn, _) = landed_rows(&rows, 0, 6);
            assert_eq!((drawn.len(), drawn[5]), (rows.len(), SidebarRow::Landing));
        }

        #[rstest]
        #[case::above_the_first_row(0, 0, more_above(list()))]
        #[case::above_the_first_row_shown(5, 5, more_above(list()))]
        #[case::below_the_last_row(0, 3, Rect::new(list().x, list().y + 3, list().width, 1))]
        fn the_line_at_an_end_goes_on_the_line_beside_it(#[case] scroll: usize, #[case] at: usize, #[case] line: Rect) {
            let n = if scroll == 0 { 3 } else { 30 };
            assert_eq!(landed_rows(&plain(n), scroll, at), (plain(n), line));
        }

        #[test]
        fn a_line_out_of_sight_is_not_drawn() {
            assert_eq!(landed_rows(&plain(30), 5, 2), (plain(30), Rect::default()));
        }

        #[test]
        fn a_line_on_the_more_label_replaces_it() {
            let names: Vec<String> = (0..20).map(|i| format!("p{i}")).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let landing = Landing { at: 8, spot: spot(Some(0), Some(8)) };
            let t = render(&dragging(view(&names), Drag::Sidebar(SidebarRow::Project(0), Some(landing))));
            let below = project_rows(list(), 1, &plain(20), 0).more_below();
            assert_eq!(row_text(&t, below).trim_end(), format!("    {}", "─".repeat(usize::from(list().width) - 5)));
        }

        fn dragging(view: View<'static>, drag: Drag) -> View<'static> {
            View { drag: Some(drag), ..view }
        }

        #[test]
        fn renders_the_landing_line_between_projects() {
            const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 22 };
            let (rows, list) = (grouped().sidebar_rows(), layout(TALL, Widths::default()).list);
            let landing = sidebar_drop(list, 1, &rows, 0, SidebarRow::Project(0), Position::new(3, list.y + 3));
            let v = dragging(grouped(), Drag::Sidebar(SidebarRow::Project(0), landing));
            insta::assert_snapshot!(render_sized(&v, TALL.width, TALL.height).backend());
        }

        #[test]
        fn the_landing_line_is_cyan() {
            let landing = Some(Landing { at: 3, spot: spot(Some(0), Some(1)) });
            let t = render(&dragging(grouped(), Drag::Sidebar(SidebarRow::Project(0), landing)));
            let cell = &t.backend().buffer()[(list().x + 5, row(3))];
            assert_eq!((cell.symbol(), cell.fg), ("─", Color::Cyan));
        }
    }

    mod agents_section {
        use super::*;

        const TALL: Rect = Rect { x: 0, y: 0, width: W, height: 30 };

        fn with_agents(sidebar: Sidebar) -> Areas {
            full_layout(TALL, Widths::default(), false, sidebar, true)
        }

        fn agent(status: Status, agent: &str, project: &str, workspace: Option<&str>, details: Details) -> AgentEntry {
            AgentEntry {
                status: Some(status),
                agent: agent.into(),
                project: project.into(),
                workspace: workspace.map(String::from),
                details,
                active: false,
            }
        }

        fn details(model: &str, percent: Option<u16>) -> Details {
            Details { model: Some(model.into()), percent, memory: None }
        }

        fn entries() -> Vec<AgentEntry> {
            vec![
                agent(
                    Status::Waiting,
                    "claude",
                    "cornercase",
                    Some("issue-98-remove-in-background"),
                    details("Opus 5.5", Some(23)),
                ),
                AgentEntry {
                    active: true,
                    ..agent(Status::Working, "codex", "website", None, details("gpt-5.5", Some(41)))
                },
                agent(Status::Done, "claude", "api", Some("main"), Details::default()),
            ]
        }

        fn shown(sidebar: Sidebar, entries: Vec<AgentEntry>) -> View<'static> {
            let folded = ProjectShape { collapsed: true, ..ProjectShape::default() };
            let tree = TreeView {
                shape: TreeShape { groups: Vec::new(), projects: vec![folded; 3], tab_bar: false },
                workspaces: Vec::new(),
            };
            View {
                has_project: true,
                issues: true,
                active: 1,
                active_tab: Some(0),
                sidebar,
                workspaces: vec![WorkspaceEntry {
                    name: "default".into(),
                    tabs: vec!["codex".into()],
                    behind: 0,
                    removing: false,
                }],
                tree: (sidebar == Sidebar::Tree).then_some(tree),
                agents: Some(AgentsView { entries, scroll: 0 }),
                ..view(&["cornercase", "website", "api"])
            }
        }

        #[rstest]
        #[case::side_by_side(Sidebar::SideBySide)]
        #[case::projects_on_top(Sidebar::ProjectsOnTop)]
        #[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
        #[case::tree(Sidebar::Tree)]
        fn sits_below_the_lists_and_above_the_footer(#[case] sidebar: Sidebar) {
            let a = with_agents(sidebar);
            let clear = |r: Rect| !r.intersects(a.agents) && !r.intersects(a.agents_border);
            assert_eq!(
                (clear(a.list), clear(a.workspaces_list), a.agents_title.y, a.agents.bottom()),
                (true, true, a.agents_border.bottom(), a.separator.y)
            );
        }

        #[rstest]
        #[case::side_by_side(Sidebar::SideBySide)]
        #[case::projects_on_top(Sidebar::ProjectsOnTop)]
        #[case::workspaces_on_top(Sidebar::WorkspacesOnTop)]
        #[case::tree(Sidebar::Tree)]
        fn leaves_the_pane_as_it_was(#[case] sidebar: Sidebar) {
            assert_eq!(with_agents(sidebar).pane, layout_with(TALL, Widths::default(), false, sidebar).pane);
        }

        #[test]
        fn takes_a_third_of_the_room_by_default() {
            assert_eq!(with_agents(Sidebar::ProjectsOnTop).agents.height, 7);
        }

        #[rstest]
        #[case::too_tall(Some(100), 10)]
        #[case::too_short(Some(1), MIN_STACK_SECTION)]
        #[case::saved(Some(8), 8)]
        fn keeps_its_height_within_both_lists_minimum(#[case] saved: Option<u16>, #[case] rows: u16) {
            let widths = Widths { agents: saved, ..Widths::default() };
            let a = full_layout(TALL, widths, false, Sidebar::ProjectsOnTop, true);
            assert_eq!((a.agents.height, a.list.height >= 1, a.workspaces_list.height >= 1), (rows, true, true));
        }

        #[test]
        fn the_stack_line_cannot_be_dragged_into_it() {
            let widths = Widths::default().stacked_dragged(Border::Stack, Position::new(5, 28), TALL, true);
            let a = full_layout(TALL, widths, false, Sidebar::ProjectsOnTop, true);
            assert_eq!((widths.stack, a.workspaces_list.bottom() <= a.agents_border.y), (Some(8), true));
        }

        #[rstest]
        #[case::side_by_side(Sidebar::SideBySide)]
        #[case::stacked(Sidebar::ProjectsOnTop)]
        #[case::tree(Sidebar::Tree)]
        fn its_line_is_a_border(#[case] sidebar: Sidebar) {
            let line = with_agents(sidebar).agents_border;
            assert_eq!(with_agents(sidebar).border_hit(Position::new(line.x + 2, line.y)), Some(Border::Agents));
        }

        #[test]
        fn a_double_click_on_its_line_resets_it() {
            assert_eq!(Widths { agents: Some(9), ..Widths::default() }.reset(Border::Agents).agents, None);
        }

        #[test]
        fn a_click_on_the_second_line_of_a_row_hits_that_agent() {
            let list = with_agents(Sidebar::ProjectsOnTop).agents_list;
            let row = agent_row(list, 1, 3, 0, 1);
            assert_eq!(agent_hit(list, 1, 3, 0, Position::new(row.x + 3, row.y + 1)), Some(1));
        }

        #[test]
        fn rows_below_the_section_are_counted() {
            let a = with_agents(Sidebar::ProjectsOnTop);
            let t = render_sized(&shown(Sidebar::ProjectsOnTop, [entries(), entries()].concat()), W, TALL.height);
            assert_eq!(row_text(&t, agent_rows(a.agents_list, 1, 6, 0).more_below()).trim(), "↓ 4 more");
        }

        #[test]
        fn the_agent_on_screen_gets_the_rail_and_the_surface() {
            let a = with_agents(Sidebar::ProjectsOnTop);
            let row = agent_row(a.agents_list, 1, 3, 0, 1);
            let t = render_sized(&shown(Sidebar::ProjectsOnTop, entries()), W, TALL.height);
            let cell = &t.backend().buffer()[(row.x, row.y + 1)];
            assert_eq!((cell.symbol(), cell.fg, cell.bg), ("▌", Color::Cyan, DARK_SURFACE));
        }

        #[test]
        fn a_hovered_row_is_filled() {
            let a = with_agents(Sidebar::ProjectsOnTop);
            let row = agent_row(a.agents_list, 1, 3, 0, 0);
            let v = View { hover: Some(Position::new(row.x + 4, row.y)), ..shown(Sidebar::ProjectsOnTop, entries()) };
            let t = render_sized(&v, W, TALL.height);
            assert_eq!(t.backend().buffer()[(row.right() - 2, row.y + 1)].bg, DARK_HOVER);
        }

        #[rstest]
        #[case::wide(30, "! cornercase › issue-98-re…")]
        #[case::without_the_workspace(18, "! cornercase")]
        #[case::cut(12, "! corner…")]
        fn a_narrow_row_drops_the_workspace_then_cuts_the_project(#[case] width: u16, #[case] expected: &str) {
            let r = Rect::new(0, 0, width, 2);
            let mut t = Terminal::new(TestBackend::new(width, 2)).expect("test backend");
            let v = shown(Sidebar::ProjectsOnTop, Vec::new());
            t.draw(|f| draw_agent(f, &v, r, 1, &entries()[0])).expect("draw");
            assert_eq!(row_text(&t, Rect { height: 1, ..r }).trim(), expected);
        }

        #[test]
        fn compact_mode_shows_it_as_a_third_menu() {
            let small = full_layout(SMALL, Widths::default(), false, Sidebar::ProjectsOnTop, true);
            let agents = small.shown(Some(Nav::Agents));
            let projects = small.shown(Some(Nav::Projects));
            assert_eq!(
                (agents.agents_list.is_empty(), agents.list.is_empty(), agents.workspaces_list.is_empty()),
                (false, true, true)
            );
            assert_eq!((agents.back.is_empty(), agents.agents_button.is_empty()), (false, true));
            assert_eq!((projects.agents_list.is_empty(), projects.agents_button.is_empty()), (true, false));
        }

        #[test]
        fn compact_mode_has_no_agents_button_when_it_is_off() {
            assert!(layout(SMALL, Widths::default()).shown(Some(Nav::Projects)).agents_button.is_empty());
        }

        #[test]
        fn renders_side_by_side() {
            insta::assert_snapshot!(render_sized(&shown(Sidebar::SideBySide, entries()), W, TALL.height).backend());
        }

        #[test]
        fn renders_projects_on_top() {
            insta::assert_snapshot!(render_sized(&shown(Sidebar::ProjectsOnTop, entries()), W, TALL.height).backend());
        }

        #[test]
        fn renders_workspaces_on_top() {
            let v = shown(Sidebar::WorkspacesOnTop, entries());
            insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
        }

        #[test]
        fn renders_the_tree() {
            insta::assert_snapshot!(render_sized(&shown(Sidebar::Tree, entries()), W, TALL.height).backend());
        }

        #[test]
        fn renders_in_a_narrow_column() {
            let v = View {
                widths: Widths { projects: MIN_COLUMN_WIDTH + 4, ..Widths::default() },
                ..shown(Sidebar::ProjectsOnTop, entries())
            };
            insta::assert_snapshot!(render_sized(&v, W, TALL.height).backend());
        }

        #[test]
        fn renders_no_agents_running() {
            insta::assert_snapshot!(render_sized(&shown(Sidebar::ProjectsOnTop, Vec::new()), W, TALL.height).backend());
        }

        #[test]
        fn renders_the_compact_agents_menu() {
            let v = View { nav: Some(Nav::Agents), ..shown(Sidebar::ProjectsOnTop, entries()) };
            insta::assert_snapshot!(render_sized(&v, SMALL.width, SMALL.height).backend());
        }

        #[test]
        fn renders_the_compact_projects_menu_with_its_button() {
            let v = View { nav: Some(Nav::Projects), ..shown(Sidebar::ProjectsOnTop, entries()) };
            insta::assert_snapshot!(render_sized(&v, SMALL.width, SMALL.height).backend());
        }

        #[test]
        fn short_terminals_do_not_panic() {
            for sidebar in Sidebar::ALL {
                for height in 1..=TALL.height {
                    render_sized(&shown(sidebar, entries()), W, height);
                }
            }
        }
    }

    mod tab_bar_drawing {
        use super::*;

        fn with_bar() -> View<'static> {
            let details = Details { model: Some("Opus 5.5".into()), percent: Some(15), memory: None };
            let claude =
                TabEntry { status: Some(Status::Working), details: details.clone(), ..TabEntry::from("claude") };
            let tabs = vec![claude, TabEntry::from("zsh")];
            View {
                has_project: true,
                workspaces: vec![WorkspaceEntry {
                    name: "main".into(),
                    tabs: tabs.clone(),
                    behind: 0,
                    removing: false,
                }],
                active_tab: Some(0),
                tab_bar: Some(tab_bar::TabBar { tabs, active: Some(0), details, scroll: 0 }),
                ..view(&["cornercase"])
            }
        }

        fn bar() -> Rect {
            layout(AREA, Widths::default()).with_tab_bar(true).tab_bar
        }

        #[test]
        fn renders_the_tabs_above_the_pane_and_none_in_the_list() {
            insta::assert_snapshot!(render(&with_bar()).backend());
        }

        #[test]
        fn the_details_row_is_muted() {
            let t = render(&with_bar());
            let r = bar();
            assert_eq!(row_text(&t, Rect { y: r.y + 1, height: 1, ..r }).trim(), "Opus 5.5 · 15%");
            assert_eq!(t.backend().buffer()[(r.x + 1, r.y + 1)].fg, Color::DarkGray);
        }

        #[test]
        fn the_active_tab_is_on_the_surface() {
            let t = render(&with_bar());
            let first = with_bar().tab_bar.expect("a bar").strip(bar()).item(0);
            assert_eq!(t.backend().buffer()[(first.x, first.y)].bg, DARK_SURFACE);
        }

        #[test]
        fn the_menu_and_close_buttons_show_only_on_hover() {
            let v = with_bar();
            let strip = v.tab_bar.as_ref().expect("a bar").strip(bar());
            let (menu, close) = (strip.menu(1), strip.close(1));
            let symbols = |v: &View| {
                let t = render(v);
                let cell = |x: u16, y: u16| t.backend().buffer()[(x, y)].symbol().to_string();
                (cell(menu.right() - 1, menu.y), cell(close.x + 1, close.y))
            };
            assert_eq!(symbols(&v), (" ".into(), " ".into()));
            let hovered = View { hover: Some(close.as_position()), ..with_bar() };
            assert_eq!(symbols(&hovered), (ROW_MENU_ICON.into(), "×".into()));
        }

        #[test]
        fn a_split_tab_shows_how_many_other_panes_it_has() {
            let mut v = with_bar();
            v.tab_bar.as_mut().expect("a bar").tabs[1].others = 2;
            let t = render(&v);
            let item = v.tab_bar.as_ref().expect("a bar").strip(bar()).item(1);
            assert_eq!(row_text(&t, item).trim_end(), " zsh +2");
            let count = t.backend().buffer()[(item.x + 5, item.y)].clone();
            assert_eq!((count.symbol(), count.fg), ("+", Color::DarkGray));
        }

        #[test]
        fn a_landing_scrolled_out_of_the_bar_draws_nothing() {
            let tabs: Vec<TabEntry> = (0..12).map(|i| TabEntry::from(format!("tab {i}"))).collect();
            let mut v = with_bar();
            v.tab_bar = Some(tab_bar::TabBar { tabs, active: Some(11), details: Details::default(), scroll: 6 });
            v.drag = Some(Drag::Bar(11, Some(0)));
            let t = render(&v);
            assert_ne!(t.backend().buffer()[(0, 0)].symbol(), "│");
        }

        #[test]
        fn without_a_project_there_is_no_bar() {
            let v = View { has_project: false, ..with_bar() };
            let t = render(&v);
            assert_eq!(row_text(&t, bar()).trim(), "");
        }
    }
}
