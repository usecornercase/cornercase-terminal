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
        ui::full_layout(area, self.widths, panel, sidebar, self.config.agents_section, self.config.icon_bar)
            .with_tab_bar(config::Tabs::from_setting(&self.config.tabs) == config::Tabs::Top)
    }

    pub(super) fn panel_shown(&self) -> bool {
        match self.side {
            Some(Side::Changes) => self.changes_shown(),
            Some(Side::Todo) => true,
            Some(Side::Files) => self.files_shown(),
            Some(Side::Flow) => self.has_run(),
            None => false,
        }
    }

    pub(super) fn show_side(&mut self, side: Option<Side>) {
        if self.side == side {
            return;
        }
        match self.side {
            Some(Side::Changes) => self.changes.close(),
            Some(Side::Todo) => self.close_todo(),
            Some(Side::Files) => self.files.close(),
            Some(Side::Flow) | None => {}
        }
        self.side = side;
    }

    pub(super) fn close_panel(&mut self) {
        self.todo.field = None;
        self.show_side(None);
    }

    pub(super) fn pane_size(&self, area: Rect) -> (u16, u16) {
        let pane = self.layout(area).pane;
        (pane.height.max(1), pane.width.max(1))
    }

    pub(super) fn visible_tab(&self) -> Option<u64> {
        self.focus().tab.filter(|_| self.nav.is_none())
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

    pub(super) fn follow(&mut self, area: Rect) {
        self.queued.area = Some(area);
        let areas = self.layout(area);
        let relayout = areas.tree != self.drawn.tree || areas.tab_bar.is_empty() != self.drawn.tab_bar.is_empty();
        self.drawn = areas;
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
            let sidebar = self.sidebar_rows();
            let rows = ui::project_rows(areas.list, areas.pitch, &sidebar, self.scroll.projects);
            if let Some(i) = self.active_row(&sidebar) {
                self.scroll.projects = rows.reveal(i);
            }
        }
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
        let tree = self.drawn.tree;
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
