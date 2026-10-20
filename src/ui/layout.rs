use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::widgets::{Block, Borders};

use super::activity;
use super::agents_section::AGENTS_LABEL;
use super::chrome::{BACK_LABEL, TODO_LABEL};
use super::files;
use super::rows::GAP;
use super::tab_bar;
use super::widgets::button_width;
use super::widths::{MIN_STACK_SECTION, PANE_PADDING};
use crate::config::Sidebar;
use crate::state::Widths;

pub const COMPACT_WIDTH: u16 = 90;

pub const COMPACT_PITCH: u16 = 3;

const STACK_FOOTER: u16 = 6;

pub(super) const COMPACT_BUTTON_WIDTH: u16 = 7;

pub(super) const HEADER_HEIGHT: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Projects,
    Workspaces,
    Stack,
    Agents,
    Changes,
}

pub(super) const STACKED_KEEP: u16 = 2 * MIN_STACK_SECTION + 1;

pub(super) fn split_agents(r: Rect, widths: Widths, keep: u16) -> [Rect; 3] {
    let room = r.height.saturating_sub(1);
    let rows = widths.agents_rows(room, keep);
    let rest = Rect { height: room - rows, ..r };
    let line = Rect { y: rest.bottom(), height: r.height.min(1), ..r };
    let agents = Rect { y: line.bottom(), height: rows, ..r };
    [rest, line, agents]
}

pub(super) fn stack_room(column: Rect, icons: bool) -> Rect {
    let y = column.y.saturating_add(HEADER_HEIGHT).min(column.bottom());
    Rect { y, height: column.bottom().saturating_sub(y).saturating_sub(stack_footer(icons) + 1), ..column }
}

fn stack_footer(icons: bool) -> u16 {
    if icons { 0 } else { STACK_FOOTER }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nav {
    Projects,
    Workspaces,
    Agents,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Areas {
    pub(crate) pitch: u16,
    pub(crate) activity: Rect,
    pub(crate) tree: bool,
    pub bar: Rect,
    pub search: Rect,
    pub(crate) search_button: Rect,
    pub back: Rect,
    pub(crate) sidebar: Rect,
    pub title: Rect,
    pub list: Rect,
    pub(crate) separator: Rect,
    pub(crate) settings: Rect,
    pub(crate) usage: Rect,
    pub quit: Rect,
    pub(crate) workspaces: Rect,
    pub(crate) workspaces_title: Rect,
    pub workspaces_list: Rect,
    pub(crate) workspaces_separator: Rect,
    pub issues: Rect,
    pub(crate) results: Rect,
    pub pane: Rect,
    pub projects_border: Rect,
    pub(crate) workspaces_border: Rect,
    pub(crate) stack_border: Rect,
    pub(crate) agents: Rect,
    pub(crate) agents_title: Rect,
    pub(crate) agents_list: Rect,
    pub(crate) agents_border: Rect,
    pub(crate) agents_button: Rect,
    pub changes: Rect,
    pub(crate) changes_border: Rect,
    pub(crate) changes_button: Rect,
    pub todo_button: Rect,
    pub(crate) files_button: Rect,
    pub(crate) tab_bar: Rect,
    pub(crate) accounts: Rect,
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

    pub(crate) fn border_hit(&self, pos: Position) -> Option<Border> {
        [Border::Projects, Border::Workspaces, Border::Stack, Border::Agents, Border::Changes]
            .into_iter()
            .find(|&b| self.border(b).contains(pos))
    }

    pub(crate) fn compact(&self) -> bool {
        !self.bar.is_empty()
    }

    pub(crate) fn todo_with_dot(&self, dot: bool) -> Rect {
        let r = self.todo_button;
        match r.x.checked_sub(2).filter(|&x| dot && !self.compact() && x > self.files_button.right()) {
            Some(x) => Rect { x, width: r.width + 2, ..r },
            None => r,
        }
    }

    #[must_use]
    pub(crate) fn with_tab_bar(self, shown: bool) -> Self {
        if !shown || self.compact() {
            return self;
        }
        let height = tab_bar::HEIGHT.min(self.pane.height);
        let pane = Rect { y: self.pane.y + height, height: self.pane.height - height, ..self.pane };
        Self { tab_bar: Rect { height, ..self.pane }, pane, ..self }
    }

    #[must_use]
    pub(crate) fn with_full_screen(self, shown: bool, area: Rect) -> Self {
        if !shown || self.changes.is_empty() {
            return self;
        }
        let area = Rect { height: area.height.saturating_sub(self.accounts.height), ..area };
        Self { changes: area, changes_border: Rect::default(), ..self }
    }

    #[must_use]
    pub(crate) fn shown(self, nav: Option<Nav>) -> Self {
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

fn projects_column(r: Rect, icons: bool) -> [Rect; 6] {
    let footer = u16::from(!icons);
    let [title, _, list, separator, settings, usage, quit] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(footer),
    ])
    .areas(r);
    [title, list, separator, settings, usage, quit]
}

fn workspaces_column(r: Rect, icons: bool) -> [Rect; 5] {
    let footer = u16::from(!icons);
    let [title, _, list, separator, issues, todo, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(footer),
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
    full_layout(area, widths, Screen { changes, sidebar, agents: false, icons: false })
}

pub(crate) fn window_layout(area: Rect, accounts: bool, layout: impl FnOnce(Rect) -> Areas) -> Areas {
    let (main, bar) = status_split(area, accounts);
    Areas { accounts: bar, ..layout(main) }
}

pub(crate) fn status_split(area: Rect, shown: bool) -> (Rect, Rect) {
    if !shown || area.height < 3 {
        return (area, Rect::default());
    }
    let height = area.height - 2;
    (Rect { height, ..area }, Rect { y: area.y + height, height: 2, ..area })
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Screen {
    pub(crate) changes: bool,
    pub(crate) sidebar: Sidebar,
    pub(crate) agents: bool,
    pub(crate) icons: bool,
}

pub(crate) fn full_layout(area: Rect, widths: Widths, screen: Screen) -> Areas {
    let Screen { changes, sidebar, agents, icons } = screen;
    if area.width < COMPACT_WIDTH {
        return compact_layout(area, changes, agents);
    }
    let columns = |r: Rect| {
        let (activity, rest) = activity_split(r, icons);
        let areas = match sidebar {
            Sidebar::SideBySide => wide_layout(rest, widths, agents, icons),
            Sidebar::Tree => tree_layout(rest, widths, agents, icons),
            Sidebar::ProjectsOnTop | Sidebar::WorkspacesOnTop => stacked_layout(rest, widths, sidebar, agents, icons),
        };
        Areas { activity, ..areas }
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

fn activity_split(r: Rect, icons: bool) -> (Rect, Rect) {
    if !icons {
        return (Rect::default(), r);
    }
    let [activity, rest] = Layout::horizontal([Constraint::Length(activity::WIDTH), Constraint::Min(0)]).areas(r);
    (activity, rest)
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

fn wide_columns(area: Rect, projects: u16, workspaces: u16) -> [Rect; 4] {
    let [columns, _, pane] = Layout::horizontal([
        Constraint::Length(projects + workspaces),
        Constraint::Length(PANE_PADDING),
        Constraint::Min(1),
    ])
    .areas(area);
    let [left, workspaces] =
        Layout::horizontal([Constraint::Length(projects), Constraint::Length(workspaces)]).areas(columns);
    [columns, left, workspaces, pane]
}

fn wide_layout(area: Rect, widths: Widths, agents: bool, icons: bool) -> Areas {
    let Widths { projects, workspaces, .. } = widths.fit(area.width);
    let [columns, left, workspaces, pane] = wide_columns(area, projects, workspaces);
    let (search, results) = search_and_results(Rect { width: columns.width.saturating_sub(1), ..columns });
    let sidebar = below_header(left);
    let [title, list, separator, settings, usage, quit] = projects_column(sidebar_block().inner(sidebar), icons);
    let lists = Rect { height: list.bottom().saturating_sub(title.y), ..title };
    let (lists, agents_areas) = agents_section(lists, widths, MIN_STACK_SECTION, agents);
    let [title, list] = if agents { section(lists) } else { [title, list] };
    let [workspaces_title, workspaces_list, workspaces_separator, issues, todo] =
        workspaces_column(below_header(sidebar_block().inner(workspaces)), icons);
    Areas {
        pitch: 1,
        search,
        search_button: search,
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
        todo_button: changes_button(todo, TODO_LABEL),
        files_button: files_button(todo),
        ..agents_areas
    }
}

fn search_and_results(r: Rect) -> (Rect, Rect) {
    let [header, results] = Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(0)]).areas(r);
    (search_area(header), results)
}

fn section(r: Rect) -> [Rect; 2] {
    let [title, _, list] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(GAP), Constraint::Min(0)]).areas(r);
    [title, list]
}

fn one_column(area: Rect, widths: Widths, keep: u16, agents: bool, icons: bool) -> (Areas, Rect) {
    let column = Rect { width: widths.stacked_width(area.width), ..area };
    let pane_x = column.right().saturating_add(PANE_PADDING).min(area.right());
    let pane = Rect { x: pane_x, width: area.right() - pane_x, ..area };
    let inner = sidebar_block().inner(column);
    let (search, results) = search_and_results(inner);
    let room = stack_room(inner, icons);
    let footer_y = inner.bottom().saturating_sub(stack_footer(icons)).max(room.y);
    let footer = Rect { y: footer_y, height: inner.bottom() - footer_y, ..inner };
    let [separator, issues, todo, settings, usage, quit] =
        if icons { [Rect::default(); 6] } else { Layout::vertical([Constraint::Length(1); 6]).areas(footer) };
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

fn stacked_layout(area: Rect, widths: Widths, sidebar: Sidebar, agents: bool, icons: bool) -> Areas {
    let (frame, sections) = one_column(area, widths, STACKED_KEEP, agents, icons);
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

fn tree_layout(area: Rect, widths: Widths, agents: bool, icons: bool) -> Areas {
    let (frame, sections) = one_column(area, widths, MIN_STACK_SECTION, agents, icons);
    let [title, list] = section(sections);
    Areas {
        tree: true,
        sidebar: Rect { x: frame.sidebar.x, width: frame.sidebar.width, ..sections },
        title,
        list,
        ..frame
    }
}

fn compact_buttons(bar: Rect) -> [Rect; 4] {
    let mut left = bar.right();
    [(); 4].map(|()| {
        let width = COMPACT_BUTTON_WIDTH.min(left.saturating_sub(bar.x));
        left -= width;
        Rect { x: left, width, ..bar }
    })
}

fn compact_agents(menu: Rect, title: Rect, agents: bool) -> [Rect; 4] {
    let [agents_title, _, agents_list] =
        Layout::vertical([Constraint::Length(COMPACT_PITCH), Constraint::Length(GAP), Constraint::Min(1)]).areas(menu);
    let width = (button_width(AGENTS_LABEL) + 2).min(title.width);
    let button = Rect { x: title.right() - width, width, ..title };
    let shown = |r: Rect| if agents { r } else { Rect::default() };
    [shown(menu), shown(agents_title), shown(agents_list), shown(button)]
}

fn compact_column(menu: Rect) -> [Rect; 5] {
    Layout::vertical([
        Constraint::Length(COMPACT_PITCH),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(COMPACT_PITCH),
    ])
    .areas(menu)
}

fn compact_layout(area: Rect, changes: bool, agents: bool) -> Areas {
    let pitch = COMPACT_PITCH;
    let [bar, below] = Layout::vertical([Constraint::Length(pitch), Constraint::Min(0)]).areas(area);
    let [search_button, todo_button, files_button, changes_button] = compact_buttons(bar);
    let [_, menu] = Layout::vertical([Constraint::Length(GAP), Constraint::Min(0)]).areas(below);
    let [title, _, list, separator, footer] = compact_column(menu);
    let [settings, usage, quit] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1), Constraint::Fill(1)]).areas(footer);
    let [workspaces_title, _, workspaces_list, workspaces_separator, issues] = compact_column(menu);
    let back = Rect { width: button_width(BACK_LABEL) + 2, ..workspaces_title }.intersection(workspaces_title);
    let [agents, agents_title, agents_list, agents_button] = compact_agents(menu, title, agents);
    Areas {
        pitch,
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
        agents,
        agents_title,
        agents_list,
        agents_button,
        changes: if changes { below } else { Rect::default() },
        changes_button,
        todo_button,
        files_button,
        ..Areas::default()
    }
}

pub fn changes_button(issues: Rect, label: &str) -> Rect {
    let width = button_width(label);
    Rect { x: issues.right().saturating_sub(width + 1), width, ..issues }.intersection(issues)
}

pub(crate) fn files_button(row: Rect) -> Rect {
    Rect { x: row.x.saturating_add(1), width: button_width(files::LABEL), ..row }.intersection(row)
}
