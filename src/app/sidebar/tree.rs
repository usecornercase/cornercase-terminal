use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::menus::Target;
use crate::error::Result;
use crate::project::Hierarchy;
use crate::ui::{self};

impl App {
    pub(in crate::app) fn reveal_in_tree(&mut self, list: Rect) {
        let Some(project) = self.project() else { return };
        let (p, w) = (self.active, project.active);
        let t = project.workspace().filter(|w| !w.tabs.is_empty()).map(|w| w.active);
        let shape = self.tree_shape();
        let rows = ui::tree_rows(&shape);
        let shown =
            [rows.iter().position(|r| *r == ui::TreeRow::Project(p)), ui::tree_active_row(&rows, &shape, (p, w, t))];
        for i in shown.into_iter().flatten() {
            self.scroll.projects = ui::tree_layout_rows(list, &shape, &rows, self.scroll.projects).reveal(i);
        }
    }

    pub(in crate::app) fn tree_open(&self) -> Vec<bool> {
        let folded = |id: u64| self.group(id).is_some_and(|g| g.collapsed);
        self.projects.iter().map(|p| !p.collapsed && !p.group.is_some_and(folded)).collect()
    }

    pub(in crate::app) fn tree_shape(&self) -> ui::TreeShape {
        let groups = self.groups.iter().map(|g| g.entry.collapsed).collect();
        let projects = self
            .projects
            .iter()
            .zip(crate::project::group_indices(&self.projects, &self.groups))
            .zip(self.tree_open())
            .map(|((p, group), open)| {
                let workspaces = p.workspaces.iter().map(|w| ui::WorkspaceShape {
                    collapsed: w.collapsed,
                    tabs: if open && !w.collapsed {
                        w.tabs.iter().map(|t| self.tab_details(t).lines()).collect()
                    } else {
                        Vec::new()
                    },
                });
                ui::ProjectShape { group, collapsed: p.collapsed, workspaces: workspaces.collect() }
            });
        ui::TreeShape { groups, projects: projects.collect(), tab_bar: !self.drawn.tab_bar.is_empty() }
    }

    pub(in crate::app) fn tree_layout(&self, list: Rect) -> ui::Rows {
        let shape = self.tree_shape();
        ui::tree_layout_rows(list, &shape, &ui::tree_rows(&shape), self.scroll.projects)
    }

    pub(in crate::app) fn tree_target(&self, row: ui::TreeRow) -> Option<Target> {
        let project = |p: usize| self.projects.get(p);
        Some(match row {
            ui::TreeRow::Group(g) => Target::Group(self.groups.get(g)?.id),
            ui::TreeRow::Project(p) => Target::Project(project(p)?.id),
            ui::TreeRow::Workspace(p, w) => {
                let project = project(p)?;
                Target::Workspace(project.id, project.workspaces.get(w)?.id)
            }
            ui::TreeRow::Tab(p, w, t) => {
                let project = project(p)?;
                let workspace = project.workspaces.get(w)?;
                Target::Tab(project.id, workspace.id, workspace.tabs.get(t)?.id)
            }
            _ => return None,
        })
    }

    pub(in crate::app) fn tree_row_of(&self, target: Target) -> Option<ui::TreeRow> {
        Some(match target {
            Target::Group(id) => ui::TreeRow::Group(self.group_index(id)?),
            Target::Project(id) => ui::TreeRow::Project(self.projects.project_index(id)?),
            Target::Workspace(project, workspace) => {
                let (p, w) = self.projects.workspace_in(project, workspace)?;
                ui::TreeRow::Workspace(p, w)
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w, t) = self.projects.tab_in(project, workspace, tab)?;
                ui::TreeRow::Tab(p, w, t)
            }
        })
    }

    pub(in crate::app) fn click_tree(&mut self, list: Rect, pos: Position, area: Rect) -> Result<()> {
        let shape = self.tree_shape();
        let scroll = self.scroll.projects;
        match self.tree_hit(list, &shape, pos) {
            Some(hit @ (ui::TreeHit::Select(row) | ui::TreeHit::Fold(row))) => {
                if let Some(target) = self.tree_target(row) {
                    let rect = ui::tree_row(list, &shape, scroll, row);
                    self.grab(target, rect, area, matches!(hit, ui::TreeHit::Fold(_)));
                }
            }
            Some(ui::TreeHit::Close(row)) => self.close_row(row),
            Some(ui::TreeHit::Menu(_)) => self.open_tree_menu(list, pos),
            Some(ui::TreeHit::NewTab(p, w)) => self.add_tab(p, w, area)?,
            Some(ui::TreeHit::NewWorkspace(p)) => self.ask_new_workspace(p),
            Some(ui::TreeHit::NewProject) => self.new_project_menu(pos),
            None => {}
        }
        Ok(())
    }

    pub(in crate::app) fn tree_hit(&self, list: Rect, shape: &ui::TreeShape, pos: Position) -> Option<ui::TreeHit> {
        ui::tree_hit(list, shape, self.scroll.projects, pos)
            .filter(|hit| !hit.workspace().is_some_and(|(p, w)| self.removing(p, w)))
    }
}
