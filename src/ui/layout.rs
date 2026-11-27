use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::widgets::{Block, Borders};

use super::activity;
use super::agents_section::AGENTS_LABEL;
use super::bots::{BOTS_LABEL, BOTS_SHORT};
use super::chrome::{BACK_LABEL, TODO_LABEL};
use super::files;
use super::rows::GAP;
use super::sections;
use super::tab_bar;
use super::widgets::button_width;
use super::widths::{Above, MIN_STACK_SECTION, PANE_PADDING};
use crate::config::Sidebar;
use crate::state::{Sections, Widths};

pub const COMPACT_WIDTH: u16 = 90;

pub const COMPACT_PITCH: u16 = 3;

const STACK_FOOTER: u16 = 6;

pub(crate) const COMPACT_BUTTON_WIDTH: u16 = 7;

pub(super) const HEADER_HEIGHT: u16 = 2;

pub(super) const PROJECTS_TITLE: &str = "Projects";

const COMPACT_BOTS_PAD: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    Projects,
    Workspaces,
    Stack,
    Agents,
    Changes,
}

pub(super) const STACKED_KEEP: u16 = 2 * MIN_STACK_SECTION + 1;

const FOLDED_STACK_ROWS: u16 = 3;

pub(super) fn split_agents(r: Rect, widths: Widths, above: Above, folded: bool) -> [Rect; 3] {
    let room = r.height.saturating_sub(1);
    let rows = widths.agents_taken(room, above, folded);
    let rest = Rect { height: room - rows, ..r };
    let line = Rect { y: rest.bottom(), height: r.height.min(1), ..r };
    let agents = Rect { y: line.bottom(), height: rows, ..r };
    [rest, line, agents]
}

pub(super) fn stack_room(column: Rect, screen: Screen) -> Rect {
    let y = column.y.saturating_add(HEADER_HEIGHT).min(column.bottom());
    Rect { y, height: column.bottom().saturating_sub(y).saturating_sub(stack_footer(screen) + 1), ..column }
}

fn stack_footer(screen: Screen) -> u16 {
    if screen.icon_bar { 0 } else { STACK_FOOTER + plugin_rows(screen) + machine_rows(screen) }
}

fn plugin_rows(screen: Screen) -> u16 {
    u16::from(screen.plugins && !screen.icon_bar)
}

fn machine_rows(screen: Screen) -> u16 {
    u16::from(screen.machine && !screen.icon_bar)
}

fn split_bottom(r: Rect, rows: u16) -> [Rect; 2] {
    let rows = rows.min(r.height);
    let top = Rect { height: r.height - rows, ..r };
    [top, Rect { y: top.bottom(), height: rows, ..r }]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nav {
    Projects,
    Workspaces,
    Agents,
    Bots,
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
    pub(crate) plugins: Rect,
    pub worktrees: Rect,
    pub(crate) machine: Rect,
    pub(crate) more_separator: Rect,
    pub more: Rect,
    pub(crate) workspaces: Rect,
    pub(crate) workspaces_title: Rect,
    pub workspaces_list: Rect,
    pub(crate) workspaces_separator: Rect,
    pub issues: Rect,
    pub(crate) results: Rect,
    pub pane: Rect,
    pub projects_border: Rect,
    pub(crate) workspaces_border: Rect,
    pub(crate) stack_line: Rect,
    pub(crate) stack_border: Rect,
    pub(crate) agents: Rect,
    pub(crate) agents_title: Rect,
    pub(crate) agents_list: Rect,
    pub(crate) agents_line: Rect,
    pub(crate) agents_border: Rect,
    pub(crate) agents_button: Rect,
    pub(crate) bots: Rect,
    pub(crate) bots_title: Rect,
    pub(crate) bots_list: Rect,
    pub(crate) bots_button: Rect,
    pub changes: Rect,
    pub(crate) changes_border: Rect,
    pub(crate) changes_button: Rect,
    pub todo_button: Rect,
    pub(crate) files_button: Rect,
    pub(crate) tab_bar: Rect,
    pub(crate) folded: Sections,
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
        if dot && !self.compact() && r.right() + 2 <= self.issues.right() {
            Rect { width: r.width + 2, ..r }
        } else {
            r
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
                plugins: hidden,
                worktrees: hidden,
                machine: hidden,
                more_separator: hidden,
                more: hidden,
                agents_button: hidden,
                bots_button: hidden,
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
                back: if matches!(nav, Some(Nav::Agents | Nav::Bots)) { projects.back } else { hidden },
                ..projects
            }
        };
        workspaces.without_sections(nav)
    }

    fn without_sections(self, nav: Option<Nav>) -> Self {
        let hidden = Rect::default();
        let agents = if nav == Some(Nav::Agents) {
            self
        } else {
            Self { agents: hidden, agents_title: hidden, agents_list: hidden, ..self }
        };
        if nav == Some(Nav::Bots) {
            agents
        } else {
            Self { bots: hidden, bots_title: hidden, bots_list: hidden, ..agents }
        }
    }

    #[must_use]
    fn with_bots(self, bots: BotsShape) -> Self {
        if !bots.page {
            let button = if bots.offered { bots_button(self.title, bots.marks, 0, 0) } else { Rect::default() };
            return Self { bots_button: button, ..self };
        }
        let hidden = Rect::default();
        let page = Rect { height: self.list.bottom().saturating_sub(self.title.y), ..self.title };
        let back = Rect { width: button_width(BACK_LABEL).min(self.title.width), ..self.title };
        Self {
            bots: page,
            bots_title: self.title,
            bots_list: self.list,
            back,
            sidebar: hidden,
            title: hidden,
            list: hidden,
            ..self
        }
    }
}

pub(super) fn bots_pad(compact: bool) -> u16 {
    if compact { COMPACT_BOTS_PAD } else { 0 }
}

fn bots_button(title: Rect, marks: u16, pad: u16, taken: u16) -> Rect {
    let room = title.width.saturating_sub(taken);
    let beside = room.saturating_sub(sections::title_width(PROJECTS_TITLE));
    let (full, short) = (button_width(BOTS_LABEL) + pad, button_width(BOTS_SHORT) + pad);
    let width = [full + marks, full, short].into_iter().find(|width| *width <= beside).unwrap_or(short).min(room);
    Rect { x: title.right().saturating_sub(taken + width), width, ..title }
}

fn right_edge(r: Rect) -> Rect {
    Rect::new(r.right().saturating_sub(1), r.y, r.width.min(1), r.height)
}

fn sidebar_block() -> Block<'static> {
    Block::default().borders(Borders::RIGHT)
}

fn projects_column(r: Rect, screen: Screen) -> [Rect; 7] {
    let footer = u16::from(!screen.icon_bar);
    let [title, _, list, separator, plugins, worktrees, machine, more] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(footer),
        Constraint::Length(plugin_rows(screen)),
        Constraint::Length(footer),
        Constraint::Length(machine_rows(screen)),
        Constraint::Length(footer),
    ])
    .areas(r);
    [title, list, separator, plugins, worktrees, machine, more]
}

fn workspaces_column(r: Rect, screen: Screen) -> [Rect; 5] {
    let footer = u16::from(!screen.icon_bar);
    let [title, _, list, separator, issues, todo, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(footer),
        Constraint::Length(plugin_rows(screen) + machine_rows(screen)),
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
    full_layout(area, widths, Screen { changes, sidebar, ..Screen::default() })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BotsShape {
    pub(crate) offered: bool,
    pub(crate) marks: u16,
    pub(crate) page: bool,
}

#[expect(clippy::struct_excessive_bools, reason = "each flag is a setting or a state the layout follows")]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Screen {
    pub(crate) changes: bool,
    pub(crate) sidebar: Sidebar,
    pub(crate) agents: bool,
    pub(crate) icon_bar: bool,
    pub(crate) plugins: bool,
    pub(crate) machine: bool,
    pub(crate) bots: BotsShape,
    pub(crate) folded: Sections,
}

impl Screen {
    pub(crate) fn wide(self) -> Self {
        if !self.bots.page {
            return self;
        }
        let folded = Sections { agents: self.folded.agents, ..Sections::default() };
        Self { agents: false, folded, ..self }
    }

    fn folds_drawn(self, workspaces: bool) -> Sections {
        Sections {
            workspaces: self.folded.workspaces && workspaces,
            agents: self.folded.agents && self.agents,
            ..self.folded
        }
    }
}

pub(crate) fn full_layout(area: Rect, widths: Widths, screen: Screen) -> Areas {
    let Screen { changes, sidebar, icon_bar: icons, .. } = screen;
    if area.width < COMPACT_WIDTH {
        return compact_layout(area, screen);
    }
    let wide = screen.wide();
    let columns = |r: Rect| {
        let (activity, rest) = activity_split(r, icons);
        let areas = match sidebar {
            Sidebar::SideBySide => wide_layout(rest, widths, wide),
            Sidebar::Tree => tree_layout(rest, widths, wide),
            Sidebar::ProjectsOnTop | Sidebar::WorkspacesOnTop => stacked_layout(rest, widths, wide),
        };
        Areas { activity, ..areas }.with_bots(screen.bots)
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

fn agents_section(r: Rect, widths: Widths, above: Above, screen: Screen) -> (Rect, Areas) {
    if !screen.agents {
        return (r, Areas::default());
    }
    let folded = screen.folded.agents;
    let [rest, line, section_rect] = split_agents(r, widths, above, folded);
    let [agents_title, agents_list] = section(section_rect, folded);
    let dragged = !folded && matches!(above, Above::Open { .. });
    let areas = Areas {
        agents: section_rect,
        agents_title,
        agents_list,
        agents_line: line,
        agents_border: line_if(dragged, line),
        ..Areas::default()
    };
    (rest, areas)
}

fn line_if(dragged: bool, line: Rect) -> Rect {
    if dragged { line } else { Rect::default() }
}

fn open_list(list: Rect, folded: bool) -> Rect {
    if folded { Rect::default() } else { list }
}

fn above_projects(folded: Sections) -> Above {
    if folded.projects { Above::Folded { rows: 1 } } else { Above::Open { keep: MIN_STACK_SECTION } }
}

fn above_stacked(folded: Sections) -> Above {
    match (folded.projects, folded.workspaces) {
        (false, false) => Above::Open { keep: STACKED_KEEP },
        (true, true) => Above::Folded { rows: FOLDED_STACK_ROWS },
        _ => Above::Open { keep: MIN_STACK_SECTION + 2 },
    }
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

fn projects_side(r: Rect, widths: Widths, screen: Screen) -> Areas {
    let folded = screen.folded;
    let [title, list, separator, plugins, worktrees, machine, more] = projects_column(r, screen);
    let lists = Rect { height: list.bottom().saturating_sub(title.y), ..title };
    let (lists, agents_areas) = agents_section(lists, widths, above_projects(folded), screen);
    let [title, list] =
        if screen.agents { section(lists, folded.projects) } else { [title, open_list(list, folded.projects)] };
    Areas { title, list, separator, plugins, worktrees, machine, more, ..agents_areas }
}

fn wide_layout(area: Rect, widths: Widths, screen: Screen) -> Areas {
    let Widths { projects, workspaces, .. } = widths.fit(area.width);
    let [columns, left, workspaces, pane] = wide_columns(area, projects, workspaces);
    let (search, results) = search_and_results(Rect { width: columns.width.saturating_sub(1), ..columns });
    let sidebar = below_header(left);
    let projects = projects_side(sidebar_block().inner(sidebar), widths, screen);
    let [workspaces_title, workspaces_list, workspaces_separator, issues, todo] =
        workspaces_column(below_header(sidebar_block().inner(workspaces)), screen);
    Areas {
        pitch: 1,
        search,
        search_button: search,
        sidebar,
        workspaces,
        workspaces_title,
        workspaces_list: open_list(workspaces_list, screen.folded.workspaces),
        workspaces_separator,
        issues,
        results,
        pane,
        projects_border: right_edge(sidebar),
        workspaces_border: right_edge(workspaces),
        todo_button: changes_button(todo, TODO_LABEL),
        files_button: files_button(todo),
        folded: screen.folds_drawn(true),
        ..projects
    }
}

fn search_and_results(r: Rect) -> (Rect, Rect) {
    let [header, results] = Layout::vertical([Constraint::Length(HEADER_HEIGHT), Constraint::Min(0)]).areas(r);
    (search_area(header), results)
}

fn section(r: Rect, folded: bool) -> [Rect; 2] {
    let [title, _, list] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(GAP), Constraint::Min(0)]).areas(r);
    [title, open_list(list, folded)]
}

fn one_column(area: Rect, widths: Widths, above: Above, screen: Screen) -> (Areas, Rect) {
    let column = Rect { width: widths.stacked_width(area.width), ..area };
    let pane_x = column.right().saturating_add(PANE_PADDING).min(area.right());
    let pane = Rect { x: pane_x, width: area.right() - pane_x, ..area };
    let inner = sidebar_block().inner(column);
    let (search, results) = search_and_results(inner);
    let room = stack_room(inner, screen);
    let footer_y = inner.bottom().saturating_sub(stack_footer(screen)).max(room.y);
    let footer = Rect { y: footer_y, height: inner.bottom() - footer_y, ..inner };
    let [separator, issues, todo, worktrees, machine, plugins, more_separator, more] =
        stack_footer_rows(footer, screen);
    let frame = Areas {
        pitch: 1,
        search,
        search_button: search,
        sidebar: column,
        separator,
        plugins,
        worktrees,
        machine,
        more_separator,
        more,
        issues,
        results,
        pane,
        projects_border: right_edge(column),
        todo_button: changes_button(todo, TODO_LABEL),
        files_button: files_button(todo),
        ..Areas::default()
    };
    let (sections, section) = agents_section(Rect { height: footer_y - room.y, ..room }, widths, above, screen);
    let frame = Areas {
        agents: Rect { x: column.x, width: column.width, ..section.agents },
        agents_title: section.agents_title,
        agents_list: section.agents_list,
        agents_line: section.agents_line,
        agents_border: section.agents_border,
        ..frame
    };
    (frame, sections)
}

fn stack_footer_rows(footer: Rect, screen: Screen) -> [Rect; 8] {
    if screen.icon_bar {
        return [Rect::default(); 8];
    }
    let row = Constraint::Length(1);
    let plugins = Constraint::Length(plugin_rows(screen));
    let machine = Constraint::Length(machine_rows(screen));
    let [separator, issues, todo, worktrees, machine, plugins, more_separator, more] =
        Layout::vertical([row, row, row, row, machine, plugins, row, row]).areas(footer);
    [separator, issues, todo, worktrees, machine, plugins, more_separator, more]
}

fn stack_split(sections: Rect, widths: Widths, folded: (bool, bool)) -> [Rect; 3] {
    let room = Rect { height: sections.height.saturating_sub(1), ..sections };
    let top_rows = widths.stack_rows(room.height, folded);
    let top = Rect { height: top_rows, ..room }.intersection(sections);
    let line = Rect { y: room.y + top_rows, height: 1, ..room }.intersection(sections);
    let under = Rect { y: room.y + top_rows + 1, height: room.height - top_rows, ..room }.intersection(sections);
    [top, line, under]
}

fn stacked_layout(area: Rect, widths: Widths, screen: Screen) -> Areas {
    let folded = screen.folded;
    let workspaces_on_top = screen.sidebar == Sidebar::WorkspacesOnTop;
    let (frame, sections) = one_column(area, widths, above_stacked(folded), screen);
    let order =
        if workspaces_on_top { (folded.workspaces, folded.projects) } else { (folded.projects, folded.workspaces) };
    let [top, line, under] = stack_split(sections, widths, order);
    let (projects, workspaces) = if workspaces_on_top { (under, top) } else { (top, under) };
    let [title, list] = section(projects, folded.projects);
    let [workspaces_title, workspaces_list] = section(workspaces, folded.workspaces);
    let widen = |r: Rect| Rect { x: frame.sidebar.x, width: frame.sidebar.width, ..r };
    Areas {
        sidebar: widen(projects),
        title,
        list,
        workspaces: widen(workspaces),
        workspaces_title,
        workspaces_list,
        stack_line: line,
        stack_border: line_if(!folded.projects && !folded.workspaces, line),
        folded: screen.folds_drawn(true),
        ..frame
    }
}

fn tree_layout(area: Rect, widths: Widths, screen: Screen) -> Areas {
    let (frame, sections) = one_column(area, widths, above_projects(screen.folded), screen);
    let [title, list] = section(sections, screen.folded.projects);
    Areas {
        tree: true,
        sidebar: Rect { x: frame.sidebar.x, width: frame.sidebar.width, ..sections },
        title,
        list,
        folded: screen.folds_drawn(false),
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

fn compact_bots(menu: Rect, title: Rect, screen: Screen) -> [Rect; 4] {
    let [bots_title, _, bots_list] =
        Layout::vertical([Constraint::Length(COMPACT_PITCH), Constraint::Length(GAP), Constraint::Min(1)]).areas(menu);
    let taken = if screen.agents { button_width(AGENTS_LABEL) + 3 } else { 0 };
    let button = bots_button(title, screen.bots.marks, COMPACT_BOTS_PAD, taken);
    let shown = |r: Rect| if screen.bots.offered { r } else { Rect::default() };
    [shown(menu), shown(bots_title), shown(bots_list), shown(button)]
}

fn compact_sections(menu: Rect, title: Rect, screen: Screen) -> Areas {
    let [agents, agents_title, agents_list, agents_button] = compact_agents(menu, title, screen.agents);
    let [bots, bots_title, bots_list, bots_button] = compact_bots(menu, title, screen);
    Areas {
        agents,
        agents_title,
        agents_list: open_list(agents_list, screen.folded.agents),
        agents_button,
        bots,
        bots_title,
        bots_list,
        bots_button,
        ..Areas::default()
    }
}

fn compact_column(menu: Rect, bands: u16) -> [Rect; 6] {
    let [title, gap, list, separator, footer] = Layout::vertical([
        Constraint::Length(COMPACT_PITCH),
        Constraint::Length(GAP),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(COMPACT_PITCH * (1 + bands)),
    ])
    .areas(menu);
    let [extra, footer] = split_bottom(footer, COMPACT_PITCH);
    [title, gap, list, separator, extra, footer]
}

fn compact_projects(menu: Rect, screen: Screen) -> [Rect; 7] {
    let (plugins, machine) = (u16::from(screen.plugins), u16::from(screen.machine));
    let [title, _, list, separator, extra, more] = compact_column(menu, 1 + plugins + machine);
    let [extra, machine] = split_bottom(extra, COMPACT_PITCH * machine);
    let [plugins, worktrees] = split_bottom(extra, COMPACT_PITCH);
    [title, list, separator, plugins, worktrees, machine, more]
}

fn compact_layout(area: Rect, screen: Screen) -> Areas {
    let changes = screen.changes;
    let pitch = COMPACT_PITCH;
    let [bar, below] = Layout::vertical([Constraint::Length(pitch), Constraint::Min(0)]).areas(area);
    let [search_button, todo_button, files_button, changes_button] = compact_buttons(bar);
    let [_, menu] = Layout::vertical([Constraint::Length(GAP), Constraint::Min(0)]).areas(below);
    let [title, list, separator, plugins, worktrees, machine, more] = compact_projects(menu, screen);
    let [workspaces_title, _, workspaces_list, workspaces_separator, _, issues] = compact_column(menu, 0);
    let back = Rect { width: button_width(BACK_LABEL) + 2, ..workspaces_title }.intersection(workspaces_title);
    Areas {
        pitch,
        bar,
        search: bar,
        search_button,
        back,
        sidebar: below,
        title,
        list: open_list(list, screen.folded.projects),
        separator,
        plugins,
        worktrees,
        machine,
        more,
        workspaces: below,
        workspaces_title,
        workspaces_list: open_list(workspaces_list, screen.folded.workspaces),
        workspaces_separator,
        issues,
        results: below,
        pane: below,
        changes: if changes { below } else { Rect::default() },
        changes_button,
        todo_button,
        files_button,
        folded: screen.folds_drawn(true),
        ..compact_sections(menu, title, screen)
    }
}

pub fn changes_button(issues: Rect, label: &str) -> Rect {
    let width = button_width(label);
    let column = issues.x.saturating_add(1).saturating_add(issues.width / 2);
    let x = column.min(issues.right().saturating_sub(width + 1)).max(issues.x);
    Rect { x, width, ..issues }.intersection(issues)
}

pub(crate) fn files_button(row: Rect) -> Rect {
    Rect { x: row.x.saturating_add(1), width: button_width(files::LABEL), ..row }.intersection(row)
}
