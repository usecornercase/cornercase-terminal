use ratatui::layout::Rect;

use super::App;
use crate::config;
use crate::project::{Project, Workspace};
use crate::ui::{self, WorkspaceRow};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Focus {
    pub(super) project: Option<u64>,
    pub(super) workspace: Option<u64>,
    pub(super) tab: Option<u64>,
}

#[derive(Default)]
pub(super) struct Scrolls {
    pub(super) projects: usize,
    pub(super) workspaces: usize,
    pub(super) agents: usize,
    pub(super) tab_bar: usize,
    pub(super) followed: Focus,
    pub(super) followed_pane: Option<u64>,
    pub(super) pane_followed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Changes,
    Todo,
    Files,
    Flow,
}

impl App {
    pub(super) fn sidebar(&self) -> config::Sidebar {
        config::Sidebar::from_setting(&self.config.sidebar)
    }

    pub(super) fn layout(&self, area: Rect) -> ui::Areas {
        let (panel, sidebar) = (self.panel_shown(), self.sidebar());
        let (agents, icons) = (self.config.agents_section, self.config.icon_bar);
        let columns =
            |main| ui::full_layout(main, self.screen.widths, ui::Screen { changes: panel, sidebar, agents, icons });
        ui::window_layout(area, self.status_bar(), columns)
            .with_tab_bar(config::Tabs::from_setting(&self.config.tabs) == config::Tabs::Top)
            .with_full_screen(self.file_full_screen(), area)
    }

    pub(super) fn status_bar(&self) -> bool {
        !self.account_pills().is_empty()
    }

    pub(super) fn above_status_bar(&self, area: Rect) -> Rect {
        ui::status_split(area, self.status_bar()).0
    }

    pub(super) fn panel_shown(&self) -> bool {
        match self.screen.side {
            Some(Side::Changes) => self.changes_shown(),
            Some(Side::Todo) => true,
            Some(Side::Files) => self.files_shown(),
            Some(Side::Flow) => self.has_run(),
            None => false,
        }
    }

    pub(super) fn show_side(&mut self, side: Option<Side>) {
        if self.screen.side == side {
            return;
        }
        match self.screen.side {
            Some(Side::Changes) => self.panels.changes.close(),
            Some(Side::Todo) => self.close_todo(),
            Some(Side::Files) => self.panels.files.close(),
            Some(Side::Flow) | None => {}
        }
        self.screen.side = side;
    }

    pub(super) fn close_panel(&mut self) {
        self.panels.todo.field = None;
        self.show_side(None);
    }

    pub(super) fn pane_size(&self, area: Rect) -> (u16, u16) {
        let pane = self.layout(area).pane;
        (pane.height.max(1), pane.width.max(1))
    }

    pub(super) fn visible_tab(&self) -> Option<u64> {
        self.focus().tab.filter(|_| self.screen.nav.is_none() && !self.file_full_screen())
    }

    pub(super) fn focus(&self) -> Focus {
        let project = self.project();
        let workspace = project.and_then(Project::workspace);
        Focus {
            project: project.map(|p| p.id),
            workspace: workspace.map(|w| w.id),
            tab: workspace.and_then(Workspace::tab).map(|t| t.id),
        }
    }

    fn follow_pane(&mut self) {
        let pane = self.term().map(|term| term.id);
        let before = std::mem::replace(&mut self.scroll.followed_pane, pane);
        let seen = std::mem::replace(&mut self.scroll.pane_followed, true);
        if seen && before != pane && pane.is_some_and(|pane| self.reveal_agent(pane)) {
            self.wire.events.snapshots.changed = true;
        }
    }

    pub(super) fn follow(&mut self, area: Rect) {
        self.panels.queued.area = Some(area);
        if self.focus() != self.scroll.followed {
            self.panels.files.set_full_screen(false);
        }
        let areas = self.layout(area);
        let relayout =
            areas.tree != self.screen.drawn.tree || areas.tab_bar.is_empty() != self.screen.drawn.tab_bar.is_empty();
        self.screen.drawn = areas;
        self.follow_pane();
        let focus = self.focus();
        let before = std::mem::replace(&mut self.scroll.followed, focus);
        if focus == before && !relayout {
            return;
        }
        if before.project.is_some() && (focus.project, focus.workspace) != (before.project, before.workspace) {
            self.unfold_focus();
        }
        self.reveal_in_tab_bar(&areas, focus.workspace != before.workspace);
        if areas.tree {
            self.reveal_in_tree(areas.list);
            return;
        }
        if focus.project != before.project || relayout {
            self.reveal_project_row(&areas);
        }
        self.reveal_workspace_rows(&areas);
    }

    fn reveal_project_row(&mut self, areas: &ui::Areas) {
        let sidebar = self.sidebar_rows();
        let rows = ui::project_rows(areas.list, areas.pitch, &sidebar, self.scroll.projects);
        if let Some(i) = self.active_row(&sidebar) {
            self.scroll.projects = rows.reveal(i);
        }
    }

    fn reveal_workspace_rows(&mut self, areas: &ui::Areas) {
        let Some(project) = self.project() else { return };
        let w = project.active;
        let mut shown = vec![WorkspaceRow::Workspace(w)];
        if let Some(workspace) = project.workspace().filter(|w| !w.tabs.is_empty()) {
            shown.push(WorkspaceRow::Tab(w, workspace.active));
        }
        let tabs = self.tab_lines();
        let rows = ui::workspace_rows(&tabs);
        for row in shown {
            if let Some(i) = rows.iter().position(|r| *r == row) {
                let layout = ui::workspace_layout(areas.workspaces_list, areas.pitch, &tabs, self.scroll.workspaces);
                self.scroll.workspaces = layout.reveal(i);
            }
        }
    }

    pub(super) fn unfold_focus(&mut self) {
        let tree = self.screen.drawn.tree;
        let Some(project) = self.projects.get_mut(self.active) else { return };
        project.collapsed = false;
        if let Some(workspace) = project.workspace_mut() {
            workspace.collapsed = false;
        }
        let group = project.group;
        if tree && let Some(entry) = group.and_then(|id| self.group_mut(id)) {
            entry.collapsed = false;
        }
    }
}
