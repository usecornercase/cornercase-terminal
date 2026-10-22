use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Clear;

mod accounts_bar;
mod activity;
mod agents_section;
pub(crate) mod changes;
mod chrome;
mod code;
pub(crate) mod combo;
mod dialog;
pub(crate) use dialog::queue as queue_form;
mod drop;
pub(crate) mod files;
pub(crate) mod flow;
mod icons;
pub(crate) mod keys;
mod layout;
mod pane;
pub(crate) mod remote;
pub(crate) mod remote_access;
mod rows;
mod sidebar;
mod star;
mod stats;
pub(crate) mod tab_bar;
mod theme;
mod toast;
pub mod todo;
mod tour;
mod tree;
mod widgets;
mod widths;
mod workspaces;

#[cfg(test)]
pub(crate) use accounts_bar::pills as pill_rects;
pub(crate) use accounts_bar::{Pill as AccountPill, Row as AccountRow, hit as account_pill_hit};
pub(crate) use activity::items as activity_items;
pub(crate) use activity::{Item as ActivityItem, Shown as ActivityShown, hit as activity_hit};
pub(crate) use agents_section::{AgentEntry, AgentsView, agent_close, agent_fold, agent_hit, agent_row, agent_rows};
pub(crate) use chrome::ChangesButton;
pub(crate) use chrome::{help_button, plugin_buttons, quit_button};
pub(crate) use dialog::flows::{
    FlowPreview, Flows, FlowsHit, flows_hit, flows_pick_rows, flows_preview, flows_row_at, flows_text, flows_wide,
    preview_body,
};
#[cfg(test)]
pub(crate) use dialog::flows::{flows_body, flows_buttons, flows_tabs};
pub(crate) use dialog::form::{Confirm, Form, FormHit, Toggle, form_hit};
pub use dialog::form::{form_area, form_buttons};
#[cfg(test)]
pub(crate) use dialog::form::{form_extra_button, form_toggle};
pub(crate) use dialog::group_style::{StyleHit, style_hit};
#[cfg(test)]
pub(crate) use dialog::group_style::{style_colour, style_done, style_icon};
pub(crate) use dialog::issues::{
    IssueRow, Issues, IssuesBody, Labels, Shown, issue_button_hit, issue_buttons_width, issue_detail, issue_name_hit,
    issues_hit, issues_list,
};
pub use dialog::issues::{issue_buttons, issues_area};
#[cfg(test)]
pub(crate) use dialog::issues::{issue_name_fields, issue_tabs, issue_toggles};
pub(crate) use dialog::menu::menu_hit;
pub use dialog::menu::{menu_area, menu_item};
pub(crate) use dialog::picker::{Entry, Picker, PickerHit, picker_hit, picker_list};
pub use dialog::picker::{picker_area, picker_buttons, picker_item};
pub(crate) use dialog::popup::{Popup, Wanted as PopupWanted, popup_close, popup_inner, popup_size};
#[cfg(test)]
pub(crate) use dialog::project_settings::project_settings_buttons;
pub(crate) use dialog::project_settings::{
    ProjectSettings, ProjectSettingsHit, project_settings_field, project_settings_hit,
};
#[cfg(test)]
pub(crate) use dialog::search::result_item;
pub(crate) use dialog::search::{ResultRow, SESSIONS_PITCH, Search, mode_button, result_hit, results_list};
pub(crate) use dialog::settings::{
    Settings, SettingsEdit, SettingsHit, SettingsLayout, SettingsPick, SettingsRow, settings_area, settings_hit,
    settings_pick_list,
};
#[cfg(test)]
pub(crate) use dialog::settings::{
    SettingsLine, line_rect, pick_list_in, settings_body, settings_done, settings_edit, settings_lines, settings_moves,
    settings_remove, settings_restart, settings_row, settings_scroll, settings_share, settings_tabs,
};
#[cfg(test)]
pub(crate) use dialog::share_settings::share_settings_buttons;
pub(crate) use dialog::share_settings::{
    ShareHit, ShareSettings, share_settings_hit, share_settings_list, share_settings_text,
};
pub(crate) use dialog::start_flow::{
    AccountField as StartAccount, Shape as StartShape, StartFlow, StartHit, start_hit, start_preview, start_prompt,
};
#[cfg(test)]
pub(crate) use dialog::start_flow::{
    start_account, start_account_list, start_branch, start_buttons, start_list, start_workspace,
};
pub(crate) use dialog::update::{Update, update_buttons, update_extra, update_notes, update_scroll};
#[cfg(test)]
pub(crate) use dialog::usage::usage_area;
pub(crate) use dialog::usage::{Usage, UsageSection, UsageWindow, usage_done, usage_share, usage_tabs};
#[cfg(test)]
pub(crate) use dialog::worktrees::worktrees_area;
pub(crate) use dialog::worktrees::{ConfirmView, WorktreeRow, Worktrees, WorktreesHit, worktrees_hit, worktrees_room};
pub(crate) use dialog::{CANCEL_LABEL, Note, Overlay};
pub(crate) use drop::{Drag, Landing, Spot, sidebar_drop, tree_drop, workspace_drop};
pub(crate) use icons::glyphs;
pub(crate) use layout::COMPACT_BUTTON_WIDTH;
pub use layout::{Areas, Border, COMPACT_PITCH, COMPACT_WIDTH, changes_button, layout, layout_with};
pub(crate) use layout::{Nav, Screen, full_layout};
pub(crate) use pane::{PaneBand, TabView, held_notice, restart_notice, resume_notice, scrollback_badge, split_footer};
pub(crate) use remote_access::{RemoteAccess, button as remote_button, done as remote_done};
pub use rows::row_close_button;
#[cfg(test)]
pub(crate) use rows::row_menu_button;
pub(crate) use rows::{MIN_MENU_ROW_WIDTH, ROW_MENU_ICON, Rows, more_above};
pub use sidebar::{NEW_BUTTON, SidebarRow, close_button, new_project_button, sidebar_rows};
pub(crate) use sidebar::{ProjectEntry, SidebarHit, active_row, entry_row, project_rows, sidebar_hit};
#[cfg(test)]
pub(crate) use star::star_buttons;
pub(crate) use star::{StarButton, star_area, star_hit};
pub(crate) use theme::{block_color, ci_icon, layer_style, muted, outcome_icon, soft};
#[cfg(test)]
pub(crate) use toast::toast_area;
pub(crate) use toast::{Toast, ToastButton, ToastIcon, toast_button};
pub(crate) use tour::{FIELD_ROWS as TOUR_FIELD_ROWS, TourButton, TourCard, tour_hit, tour_layout};
pub(crate) use tree::{
    ProjectShape, TreeHit, TreeRow, TreeShape, TreeView, WorkspaceShape, tree_active_row, tree_hit, tree_layout_rows,
    tree_row, tree_rows, tree_tag,
};
#[cfg(test)]
pub(crate) use tree::{tree_arrow, tree_close};
pub(crate) use widgets::update_button;
pub use widths::SIDEBAR_WIDTH;
#[cfg(test)]
pub(crate) use widths::{MIN_COLUMN_WIDTH, MIN_PANE_WIDTH, MIN_STACK_SECTION, PANE_PADDING, WORKSPACES_WIDTH};
pub use workspaces::{Details, TabLines, WorkspaceRow, new_workspace_button, workspace_row};
pub(crate) use workspaces::{
    ICON_WIDTH, REOPEN_ICON, TabEntry, Tag, WorkspaceEntry, WorkspaceHit, workspace_hit, workspace_layout,
    workspace_rows, workspace_tag,
};

use crate::agents::activity::Status;
use crate::config::Sidebar;
use crate::plugins::manifest::Face;
use crate::project::group::GroupEntry;
use crate::state::Widths;

use agents_section::draw_agents;
use chrome::{
    USAGE_LABEL, draw_bar, draw_border, draw_borders, draw_column_border, draw_footer, draw_issues_row,
    draw_plugin_buttons, draw_search_bar,
};
use dialog::flows::draw_flows;
use dialog::form::{draw_confirm, draw_form};
use dialog::group_style::draw_group_style;
use dialog::issues::draw_issues;
use dialog::menu::draw_menu;
use dialog::picker::draw_picker;
use dialog::project_settings::draw_project_settings;
use dialog::search::draw_results;
use dialog::settings::draw_settings;
use dialog::share_settings::draw_share_settings;
use dialog::start_flow::draw_start_flow;
use dialog::update::draw_update;
use dialog::usage::draw_usage;
use dialog::worktrees::draw_worktrees;
use dialog::{FORM_WIDTH, draw_submit, form_inner};
#[cfg(test)]
pub(crate) use pane::NoTabButtons;
pub(crate) use pane::{NoTab, no_tab_buttons, welcome_button};
use pane::{draw_no_tab, draw_tab, draw_welcome};
use rows::{GAP, Zone, draw_more};
use sidebar::draw_sidebar;
use star::draw_star;
use theme::{BRAND_COLOR, hover_colour, line_colour, surface_colour};
use toast::draw_toast;
use tour::{draw_pointer, draw_tour};
use tree::draw_tree;
use widgets::{
    action_style, button_style, button_width, centered, dim, draw_band, draw_close, draw_separator, hovered,
    hovered_at, middle, overlay_block, put, rail, sidebar_hovered, tab_style,
};
use widgets::{fit, scroll_edge};
use workspaces::{DetailLine, draw_details, draw_workspaces};

#[expect(clippy::struct_excessive_bools, reason = "each flag is a state or a setting the drawing follows")]
#[derive(Default)]
pub(crate) struct View<'a> {
    pub(crate) groups: Vec<GroupEntry>,
    pub(crate) projects: Vec<ProjectEntry>,
    pub(crate) active: usize,
    pub(crate) projects_scroll: usize,
    pub(crate) has_project: bool,
    pub(crate) workspaces: Vec<WorkspaceEntry>,
    pub(crate) active_workspace: usize,
    pub(crate) active_tab: Option<usize>,
    pub(crate) workspaces_scroll: usize,
    pub(crate) issues: bool,
    pub(crate) hover: Option<Position>,
    pub(crate) widths: Widths,
    pub(crate) sidebar: Sidebar,
    pub(crate) resizing: Option<Border>,
    pub(crate) light: bool,
    pub(crate) muted: Color,
    pub(crate) soft: Color,
    pub(crate) tab: Option<TabView>,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) toast: Option<Toast<'a>>,
    pub(crate) star: bool,
    pub(crate) tour: Option<TourCard>,
    pub(crate) nav: Option<Nav>,
    pub(crate) update: Option<String>,
    pub(crate) remote: Option<bool>,
    pub(crate) changes: Option<changes::View>,
    pub(crate) changes_button: Option<ChangesButton>,
    pub(crate) todo: Option<todo::View>,
    pub(crate) flow: Option<flow::View>,
    pub(crate) flow_run: bool,
    pub(crate) todo_dot: bool,
    pub(crate) todo_pending: bool,
    pub(crate) files: Option<files::View>,
    pub(crate) attention: Option<Status>,
    pub(crate) spin: Option<u8>,
    pub(crate) drag: Option<Drag>,
    pub(crate) tree: Option<TreeView>,
    pub(crate) agents: Option<AgentsView>,
    pub(crate) counts: bool,
    pub(crate) icon_bar: bool,
    pub(crate) icons: bool,
    pub(crate) no_tab: NoTab,
    pub(crate) tab_bar: Option<tab_bar::TabBar>,
    pub(crate) plugin_buttons: Vec<Face>,
}

impl View<'_> {
    pub(crate) fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let groups: Vec<Option<usize>> = self.projects.iter().map(|p| p.group).collect();
        let collapsed: Vec<bool> = self.groups.iter().map(|g| g.collapsed).collect();
        sidebar_rows(&groups, &collapsed)
    }

    pub(crate) fn tab_lines(&self) -> TabLines {
        let lines = self.workspaces.iter().map(|w| w.tabs.iter().map(|t| t.details.lines()).collect()).collect();
        TabLines::new(lines, self.tab_bar.is_none()).folded(self.workspaces.iter().map(|w| w.collapsed).collect())
    }

    fn glyphs(&self) -> &'static icons::Glyphs {
        glyphs(self.icons)
    }

    pub(crate) fn with_icons(mut self, icons: bool) -> Self {
        self.icons = icons;
        if let Some(files) = &mut self.files {
            files.icons = icons;
        }
        if let Some(changes) = &mut self.changes {
            changes.icons = icons;
        }
        self
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

fn draw_main(f: &mut Frame, view: &View, areas: &Areas) {
    if let Some(bar) = view.tab_bar.as_ref().filter(|_| view.has_project && !areas.tab_bar.is_empty()) {
        tab_bar::draw(f, view, bar, areas.tab_bar);
    }
    draw_tab_or_empty(f, view, areas);
    draw_chrome(f, view, areas);
    if areas.tree {
        draw_tree(f, view, areas);
    } else if !areas.sidebar.is_empty() {
        draw_sidebar(f, view, areas);
    }
    if let Some(agents) = &view.agents {
        draw_agents(f, view, agents, areas);
    }
    if [areas.separator, areas.settings, areas.usage, areas.quit].iter().any(|r| !r.is_empty()) {
        draw_footer(f, view, areas);
    }
    if !areas.plugins.is_empty() {
        draw_plugin_buttons(f, view, areas.plugins);
    }
    if !areas.workspaces.is_empty() {
        draw_workspaces(f, view, areas);
    }
    if view.has_project && [areas.workspaces_separator, areas.issues].iter().any(|r| !r.is_empty()) {
        draw_issues_row(f, view, areas);
    }
}

fn draw_tab_or_empty(f: &mut Frame, view: &View, areas: &Areas) {
    match &view.tab {
        Some(tab) => draw_tab(f, view, tab, areas.pane),
        None if view.has_project => draw_no_tab(f, view, areas.pane),
        None if view.projects.is_empty() => draw_welcome(f, view, areas.pane),
        None => {}
    }
}

fn draw_chrome(f: &mut Frame, view: &View, areas: &Areas) {
    if areas.compact() {
        draw_bar(f, view, areas);
        if view.nav.is_some() {
            f.render_widget(Clear, areas.pane);
        }
    } else {
        draw_column_border(f, view.line(), areas.projects_border);
        draw_column_border(f, view.line(), areas.workspaces_border);
        draw_separator(f, view.line(), areas.stack_border);
        draw_borders(f, view, areas);
        draw_search_bar(f, view, areas.search);
        activity::draw(f, view, areas.activity);
    }
}

pub(crate) fn draw(f: &mut Frame, view: &View) {
    let panel = view.changes.is_some() || view.todo.is_some() || view.files.is_some() || view.flow.is_some();
    let full = view.files.as_ref().is_some_and(files::View::full_screen);
    let (agents, icon_bar) = (view.agents.is_some(), view.icon_bar);
    let plugins = !view.plugin_buttons.is_empty();
    let screen = Screen { changes: panel, sidebar: view.sidebar, agents, icon_bar, plugins };
    let areas = full_layout(f.area(), view.widths, screen)
        .with_tab_bar(view.tab_bar.is_some())
        .with_full_screen(full, f.area())
        .shown(view.nav);
    if !full {
        draw_main(f, view, &areas);
    }
    if panel && !areas.changes.is_empty() {
        draw_side_panel(f, view, &areas);
    }
    if let Some(card) = &view.tour {
        draw_pointer(f, card);
    }
    if !full {
        activity::draw_tooltip(f, view, areas.activity);
    }
    if view.overlay.as_ref().is_some_and(Overlay::is_modal) {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::default().add_modifier(Modifier::DIM));
    }
    draw_overlay(f, view, &areas);
    if view.star {
        draw_star(f, view);
    }
    if let Some(card) = &view.tour {
        draw_tour(f, view, card);
    }
    if let Some(toast) = view.toast {
        draw_toast(f, view, toast);
    }
}

fn draw_side_panel(f: &mut Frame, view: &View, areas: &Areas) {
    f.render_widget(Clear, areas.changes);
    let border = areas.changes_border;
    draw_column_border(f, view.line(), border);
    draw_border(f, view, border, Border::Changes);
    let hover = view.hover.filter(|_| view.overlay.is_none());
    match (&view.changes, &view.todo, &view.files, &view.flow) {
        (Some(changes), ..) => changes::draw(f, areas.changes, changes, hover.filter(|_| view.drag.is_none())),
        (None, Some(todo), ..) => todo::draw(f, areas.changes, todo, hover),
        (None, None, Some(files), _) => files::draw(f, areas.changes, files, hover),
        (None, None, None, Some(flow)) => flow::draw(f, areas.changes, flow, hover.filter(|_| view.drag.is_none())),
        (None, None, None, None) => {}
    }
}

fn draw_overlay(f: &mut Frame, view: &View, areas: &Areas) {
    match &view.overlay {
        Some(Overlay::Menu { at, items, disabled }) => draw_menu(f, view, *at, items, disabled),
        Some(Overlay::Form(form)) => draw_form(f, view, form),
        Some(Overlay::Queue(form)) => dialog::queue::draw(f, view, form),
        Some(Overlay::GroupStyle(group)) => draw_group_style(f, view, group),
        Some(Overlay::Confirm(confirm)) => draw_confirm(f, view, confirm),
        Some(Overlay::Update(update)) => draw_update(f, view, update),
        Some(Overlay::Usage(usage)) => draw_usage(f, view, usage),
        Some(Overlay::Remote(remote)) => remote_access::draw(f, view, remote),
        Some(Overlay::Picker(picker)) => draw_picker(f, view, picker),
        Some(Overlay::Issues(issues)) => draw_issues(f, view, issues),
        Some(Overlay::Settings(settings)) => draw_settings(f, view, settings),
        Some(Overlay::ShareSettings(share)) => draw_share_settings(f, view, share),
        Some(Overlay::Flows(flows)) => draw_flows(f, view, flows),
        Some(Overlay::StartFlow(start)) => draw_start_flow(f, view, start),
        Some(Overlay::ProjectSettings(settings)) => draw_project_settings(f, view, settings),
        Some(Overlay::Worktrees(worktrees)) => draw_worktrees(f, view, worktrees),
        Some(Overlay::Popup(popup)) => dialog::popup::draw(f, view, popup),
        Some(Overlay::Search(search)) if search.showing_results() => draw_results(f, view, search, areas.results),
        Some(Overlay::Keys(menu)) => keys::draw(f, view, menu, areas.pane),
        Some(Overlay::Search(_)) | None => {}
    }
}

pub(crate) fn status_text(status: Status) -> String {
    theme::status_icon(Color::Reset, status, None).content.into_owned()
}

#[cfg(test)]
mod tests;
