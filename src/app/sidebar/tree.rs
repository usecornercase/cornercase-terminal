use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::layout::List;
use crate::app::machines::{self, RemoteProject};
use crate::app::menus::Target;
use crate::error::Result;
use crate::project::{Hierarchy, Project};
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
            self.scroll.set(
                List::Projects,
                ui::tree_layout_rows(list, &shape, &rows, self.scroll.of(List::Projects)).reveal(i),
            );
        }
    }

    pub(in crate::app) fn tree_open(&self) -> Vec<bool> {
        let folded = |id: u64| self.group(id).is_some_and(|g| g.collapsed);
        self.projects.iter().map(|p| !p.collapsed && !p.group.is_some_and(folded)).collect()
    }

    pub(in crate::app) fn tree_shape(&self) -> ui::TreeShape {
        let remote = self.remote_projects();
        let groups = self.groups.iter().map(|g| g.entry.collapsed).collect();
        let indices = crate::project::group_indices(&self.projects, &self.groups);
        let ours = self.projects.iter().zip(indices).zip(self.tree_open());
        let ours = ours.map(|((p, group), open)| self.project_shape(p, group, open, &remote));
        let theirs = self.alone_entries(&remote).map(|entries| machines::machine_project(&entries));
        let projects = ours.chain(theirs).collect();
        ui::TreeShape { groups, projects, tab_bar: !self.screen.drawn.tab_bar.is_empty() }
    }

    fn project_shape(
        &self,
        p: &Project,
        group: Option<usize>,
        open: bool,
        remote: &[RemoteProject],
    ) -> ui::ProjectShape {
        let ours = p.workspaces.iter().map(|w| ui::WorkspaceShape {
            collapsed: w.collapsed,
            machine: false,
            tabs: if open && !w.collapsed {
                w.tabs.iter().map(|t| self.tab_details(t).lines()).collect()
            } else {
                Vec::new()
            },
        });
        let theirs = self.paired_entries(remote, p.id);
        let workspaces = ours.chain(theirs.iter().map(|entry| machines::machine_shape(entry, open))).collect();
        ui::ProjectShape { group, collapsed: p.collapsed, machine: false, workspaces }
    }

    pub(in crate::app) fn tree_layout(&self, list: Rect) -> ui::Rows {
        let shape = self.tree_shape();
        ui::tree_layout_rows(list, &shape, &ui::tree_rows(&shape), self.scroll.of(List::Projects))
    }

    pub(in crate::app) fn tree_target(&self, row: ui::TreeRow) -> Option<Target> {
        Some(match row {
            ui::TreeRow::Group(g) => Target::Group(self.groups.get(g)?.id),
            ui::TreeRow::Project(p) => Target::Project(self.projects.get(p)?.id),
            ui::TreeRow::Workspace(p, w) => self.workspace_target(self.projects.workspace_at(p, w)?),
            ui::TreeRow::Tab(p, w, t) => self.tab_target(self.projects.tab_at(p, w, t)?),
            _ => return None,
        })
    }

    pub(in crate::app) fn tree_row_of(&self, target: Target) -> Option<ui::TreeRow> {
        Some(match target {
            Target::Group(id) => ui::TreeRow::Group(self.group_index(id)?),
            Target::Project(id) => ui::TreeRow::Project(self.projects.project_index(id)?),
            Target::Workspace(project, workspace) => {
                let at = self.projects.workspace_in(project, workspace)?;
                ui::TreeRow::Workspace(at.project(), at.index())
            }
            Target::Tab(project, workspace, tab) => {
                let at = self.projects.tab_in(project, workspace, tab)?;
                ui::TreeRow::Tab(at.project(), at.workspace().index(), at.index())
            }
        })
    }

    pub(in crate::app) fn click_tree(&mut self, list: Rect, pos: Position, area: Rect) -> Result<()> {
        let shape = self.tree_shape();
        let scroll = self.scroll.of(List::Projects);
        let hit = self.tree_hit(list, &shape, pos);
        if self.press_remote_tree(list, &shape, hit, area) {
            return Ok(());
        }
        if let Some(ui::TreeHit::Select(ui::TreeRow::Workspace(p, w))) = hit
            && let Some(tag) = self.tag_in_tree(list, &shape, (p, w), pos)
        {
            self.press_tag(tag, area);
            return Ok(());
        }
        match hit {
            Some(hit @ (ui::TreeHit::Select(row) | ui::TreeHit::Fold(row))) => {
                if let Some(target) = self.tree_target(row) {
                    let rect = ui::tree_row(list, &shape, scroll, row);
                    self.grab(target, rect, area, matches!(hit, ui::TreeHit::Fold(_)));
                }
            }
            Some(ui::TreeHit::Close(row)) => self.close_row(row),
            Some(ui::TreeHit::Menu(_)) => self.open_tree_menu(list, pos),
            Some(ui::TreeHit::NewTab(p, w)) => self.add_tab_row(p, w, area)?,
            Some(ui::TreeHit::NewWorkspace(p)) => self.new_workspace_menu(p, pos),
            Some(ui::TreeHit::NewProject) => self.new_project_menu(pos),
            None => {}
        }
        Ok(())
    }

    pub(in crate::app) fn tree_hit(&self, list: Rect, shape: &ui::TreeShape, pos: Position) -> Option<ui::TreeHit> {
        ui::tree_hit(list, shape, self.scroll.of(List::Projects), pos)
            .filter(|hit| !hit.workspace().is_some_and(|(p, w)| self.removing(p, w)))
    }
}
