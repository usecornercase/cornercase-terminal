use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Clear;

mod agents_section;
pub mod changes;
mod chrome;
mod dialog;
mod drop;
pub mod files;
pub mod keys;
mod layout;
mod pane;
pub mod remote;
pub mod remote_access;
mod rows;
mod sidebar;
mod stats;
pub mod tab_bar;
mod theme;
mod toast;
pub mod todo;
mod tree;
mod widgets;
mod widths;
mod workspaces;

pub use agents_section::{AGENTS_LABEL, AgentEntry, AgentsView, agent_hit, agent_row, agent_rows};
pub use chrome::{ChangesButton, TODO_LABEL};
pub use dialog::form::{
    Confirm, Form, FormHit, Toggle, form_area, form_buttons, form_extra_button, form_hit, form_toggle,
};
pub use dialog::group_style::{StyleHit, style_colour, style_done, style_hit, style_icon};
pub use dialog::issues::{
    IssueRow, Issues, IssuesBody, issue_button_hit, issue_buttons, issue_detail, issue_tabs, issue_toggles,
    issues_area, issues_hit, issues_list,
};
pub use dialog::menu::{menu_area, menu_hit, menu_item};
pub use dialog::picker::{
    Entry, Picker, PickerHit, list_item_at, picker_area, picker_buttons, picker_hit, picker_item, picker_list,
};
pub use dialog::search::{ResultRow, Search, result_hit, result_item, results_list};
pub use dialog::settings::{
    Settings, SettingsEdit, SettingsHit, SettingsLayout, SettingsLine, SettingsPick, SettingsRow, settings_area,
    settings_body, settings_done, settings_edit, settings_hit, settings_lines, settings_moves, settings_pick_list,
    settings_remove, settings_restart, settings_row, settings_scroll, settings_tabs,
};
pub use dialog::update::{Update, update_buttons, update_notes, update_scroll};
pub use dialog::usage::{Usage, UsageSection, UsageWindow, usage_area, usage_done, usage_share, usage_tabs};
pub use dialog::{CANCEL_LABEL, Note, Overlay};
pub use drop::{Drag, Landing, Spot, sidebar_drop, tree_drop, workspace_drop};
pub use layout::{
    Areas, Border, COMPACT_PITCH, COMPACT_WIDTH, Nav, Sidebar, Tabs, changes_button, files_button, full_layout, layout,
    layout_with,
};
pub use pane::{TabView, scrollback_badge, scrollback_label};
pub use remote_access::{RemoteAccess, button as remote_button, done as remote_done};
pub use rows::{ROW_MENU_ICON, Rows, more_above, row_close_button, row_menu_button};
pub use sidebar::{
    GROUP_COLOURS, GROUP_ICONS, GROUP_STYLES, GroupEntry, NEW_BUTTON, ProjectEntry, SidebarHit, SidebarRow, active_row,
    close_button, entry_row, new_project_button, project_rows, sidebar_hit, sidebar_rows,
};
pub use theme::muted;
pub use toast::{Toast, ToastButton, ToastIcon, toast_area, toast_button};
pub use tree::{
    ProjectShape, TreeHit, TreeRow, TreeShape, TreeView, WorkspaceShape, tree_active_row, tree_arrow, tree_close,
    tree_hit, tree_layout_rows, tree_row, tree_rows,
};
pub use widgets::update_button;
pub use widths::{
    MIN_COLUMN_WIDTH, MIN_PANE_WIDTH, MIN_STACK_SECTION, PANE_PADDING, SIDEBAR_WIDTH, WORKSPACES_WIDTH, Widths,
};
pub use workspaces::{
    Details, ICON_WIDTH, TabEntry, TabLines, WorkspaceEntry, WorkspaceHit, WorkspaceRow, new_workspace_button,
    workspace_hit, workspace_layout, workspace_row, workspace_rows,
};

use crate::activity::Status;

pub use crate::issues::browser::IssuesHit;

use agents_section::draw_agents;
use chrome::{
    USAGE_LABEL, draw_bar, draw_border, draw_borders, draw_column_border, draw_footer, draw_issues_row, draw_search_bar,
};
use dialog::form::{draw_confirm, draw_form};
use dialog::group_style::draw_group_style;
use dialog::issues::draw_issues;
use dialog::menu::draw_menu;
use dialog::picker::draw_picker;
use dialog::search::draw_results;
use dialog::settings::draw_settings;
use dialog::update::draw_update;
use dialog::usage::draw_usage;
use dialog::{FORM_WIDTH, draw_submit, form_inner};
use pane::{draw_no_tab, draw_tab, draw_welcome};
use rows::{GAP, Zone, draw_more};
use sidebar::draw_sidebar;
use theme::{BRAND_COLOR, hover_colour, line_colour, surface_colour};
use toast::draw_toast;
use tree::draw_tree;
use widgets::{
    action_style, button_style, button_width, centered, dim, draw_band, draw_close, draw_separator, hovered,
    hovered_at, middle, overlay_block, put, rail, sidebar_hovered, tab_style,
};
use workspaces::{draw_details, draw_workspaces};

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

#[cfg(test)]
mod tests;
