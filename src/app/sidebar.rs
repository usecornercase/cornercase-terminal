use ratatui::layout::{Position, Rect};

use super::App;
use super::layout::List;
use super::machine_links;
use super::menus::MenuAction;
use super::menus::Target;
use super::overlay::Overlay;
use super::plugins::Spot;
use crate::editor::Editor;
use crate::error::Result;
use crate::git;
use crate::project::{Hierarchy, Workspace};
use crate::ui::{self, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};

mod agents;
mod drag;
mod tree;

impl App {
    pub(super) fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let collapsed: Vec<bool> = self.groups.iter().map(|g| g.entry.collapsed).collect();
        let rows = ui::sidebar_rows(&crate::project::group_indices(&self.projects, &self.groups), &collapsed);
        let ours = self.projects.len();
        ui::machine_rows(rows, ours..ours + machine_links::remote_only(&self.remote_projects()).count())
    }

    pub(super) fn active_row(&self, sidebar: &[SidebarRow]) -> Option<usize> {
        let group = self.project().and_then(|p| p.group).and_then(|id| self.group_index(id));
        ui::active_row(sidebar, self.active, group)
    }

    pub(super) fn click_projects(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) {
        let rows = self.sidebar_rows();
        if self.press_remote_project(list, pitch, &rows, (pos, area)) {
            return;
        }
        let rect = |row: SidebarRow| ui::entry_row(list, pitch, &rows, self.scroll.of(List::Projects), row);
        match self.sidebar_hit(list, pitch, &rows, pos) {
            Some(SidebarHit::Select(i)) => {
                self.grab(Target::Project(self.projects[i].id), rect(SidebarRow::Project(i)), area, false);
            }
            Some(SidebarHit::Close(i)) => self.close_row(ui::TreeRow::Project(i)),
            Some(SidebarHit::Group(g)) => {
                self.grab(Target::Group(self.groups[g].id), rect(SidebarRow::Group(g)), area, false);
            }
            Some(SidebarHit::New) => self.new_project_menu(pos),
            Some(SidebarHit::CloseGroup(g)) => self.close_row(ui::TreeRow::Group(g)),
            Some(SidebarHit::Menu(_) | SidebarHit::GroupMenu(_)) => self.open_project_menu(list, pitch, pos),
            None => {}
        }
    }

    pub(super) fn new_project_menu(&mut self, pos: Position) {
        self.overlay = Some(Overlay::Menu { at: pos, actions: vec![MenuAction::OpenProject, MenuAction::NewGroup] });
    }

    pub(super) fn new_workspace_menu(&mut self, p: usize, pos: Position) {
        let project = self.projects[p].id;
        let actions = vec![MenuAction::NewWorkspace(project), MenuAction::StartFlow(project)];
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    pub(super) fn sidebar_layout(&self, areas: &ui::Areas) -> ui::Rows {
        if areas.tree {
            self.tree_layout(areas.list)
        } else {
            ui::project_rows(areas.list, areas.pitch, &self.sidebar_rows(), self.scroll.of(List::Projects))
        }
    }

    pub(super) fn close_row(&mut self, row: ui::TreeRow) {
        match row {
            ui::TreeRow::Group(g) => self.overlay = Some(Overlay::DeleteGroup { group: self.groups[g].id }),
            ui::TreeRow::Project(p) => self.overlay = Some(Overlay::CloseProject { project: self.projects[p].id }),
            ui::TreeRow::Workspace(p, w) => self.close_workspace_row(p, w, false),
            ui::TreeRow::Tab(p, w, t) => self.close_tab_row((p, w, t), false),
            _ => {}
        }
    }

    pub(super) fn ask_new_workspace(&mut self, p: usize) {
        let project = &self.projects[p];
        let worktree = git::is_repo_root(&project.path).then_some(true);
        self.overlay = Some(Overlay::NewWorkspace {
            project: project.id,
            input: Editor::default(),
            worktree,
            error: None,
            creating: false,
            run_on: self.run_on_for(project.id),
        });
    }

    pub(super) fn tab_lines(&self) -> ui::TabLines {
        let lines = |w: &Workspace| -> Vec<u16> { w.tabs.iter().map(|t| self.tab_details(t).lines()).collect() };
        let remote = self.remote_projects();
        let column = self.column(&remote);
        let (ours, theirs) = (column.ours, self.remote_entries(&column));
        let lines = ours.iter().map(lines).chain(theirs.iter().map(ui::WorkspaceEntry::tab_lines)).collect();
        let folded = ours.iter().map(|w| w.collapsed).chain(theirs.iter().map(|w| w.collapsed)).collect();
        let bare = ours.iter().map(|_| false).chain(theirs.iter().map(|_| true)).collect();
        ui::TabLines::new(lines, self.screen.drawn.tab_bar.is_empty()).folded(folded).bare(bare)
    }

    pub(super) fn click_workspaces(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) -> Result<()> {
        let tabs = self.tab_lines();
        if self.press_remote_row(list, pitch, &tabs, (pos, area)) || self.project().is_none() {
            return Ok(());
        }
        let hit = self.workspace_hit(list, pitch, &tabs, pos);
        if matches!(hit, Some(WorkspaceHit::NewTab(_))) {
            self.screen.nav = None;
        }
        if let Some(WorkspaceHit::Workspace(w)) = hit
            && let Some(tag) = self.tag_in_list(list, pitch, &tabs, (w, pos))
        {
            self.press_tag(tag, area);
            return Ok(());
        }
        let Some(hit) = hit else { return Ok(()) };
        let (p, ask) = (self.active, pitch > 1);
        let rect = |row: WorkspaceRow| ui::workspace_row(list, pitch, &tabs, self.scroll.of(List::Workspaces), row);
        let target = self.workspace_hit_target(p, hit);
        match (hit, target) {
            (WorkspaceHit::Workspace(w) | WorkspaceHit::Fold(w), Some(target)) => {
                let fold = matches!(hit, WorkspaceHit::Fold(_));
                self.grab(target, rect(WorkspaceRow::Workspace(w)), area, fold);
            }
            (WorkspaceHit::Tab(w, t), Some(target)) => self.grab(target, rect(WorkspaceRow::Tab(w, t)), area, false),
            (WorkspaceHit::CloseWorkspace(w), _) => self.close_workspace_row(p, w, ask),
            (WorkspaceHit::CloseTab(w, t), _) => self.close_tab_row((p, w, t), ask),
            (WorkspaceHit::NewTab(w), _) => return self.add_tab_row(p, w, area),
            (WorkspaceHit::NewWorkspace, _) => self.new_workspace_menu(p, pos),
            (WorkspaceHit::WorkspaceMenu(_) | WorkspaceHit::TabMenu(..), _) => {
                self.open_workspace_menu(list, pitch, pos);
            }
            _ => {}
        }
        Ok(())
    }

    fn workspace_hit_target(&self, p: usize, hit: WorkspaceHit) -> Option<Target> {
        match hit {
            WorkspaceHit::Workspace(w)
            | WorkspaceHit::Fold(w)
            | WorkspaceHit::CloseWorkspace(w)
            | WorkspaceHit::WorkspaceMenu(w) => Some(self.workspace_target(self.projects.workspace_at(p, w)?)),
            WorkspaceHit::Tab(w, t) | WorkspaceHit::CloseTab(w, t) | WorkspaceHit::TabMenu(w, t) => {
                Some(self.tab_target(self.projects.tab_at(p, w, t)?))
            }
            WorkspaceHit::NewTab(_) | WorkspaceHit::NewWorkspace => None,
        }
    }

    pub(super) fn add_tab_row(&mut self, p: usize, w: usize, area: Rect) -> Result<()> {
        match self.projects.workspace_at(p, w) {
            Some(at) => self.add_tab(at, area),
            None => Ok(()),
        }
    }

    fn close_workspace_row(&mut self, p: usize, w: usize, ask: bool) {
        let Some(at) = self.projects.workspace_at(p, w) else { return };
        let workspace = self.projects.workspace(at);
        if (ask || self.agents.closed.count(workspace.id) > 0) && !workspace.worktree {
            self.overlay = Some(Overlay::CloseWorkspace { project: self.projects[p].id, workspace: workspace.id });
        } else {
            self.close_workspace(at);
        }
    }

    pub(super) fn close_tab_row(&mut self, (p, w, t): (usize, usize, usize), ask: bool) {
        let Some(at) = self.projects.tab_at(p, w, t) else { return };
        if !ask {
            return self.close_tab(at);
        }
        let workspace = self.projects.workspace(at.workspace()).id;
        let (project, tab) = (self.projects[p].id, self.projects.tab(at).id);
        self.overlay = Some(Overlay::CloseTab { project, workspace, tab });
    }

    pub(super) fn open_project_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let actions = match self.sidebar_hit(list, pitch, &self.sidebar_rows(), pos) {
            Some(SidebarHit::Select(i) | SidebarHit::Close(i) | SidebarHit::Menu(i)) => self.project_menu(i),
            Some(SidebarHit::Group(g) | SidebarHit::CloseGroup(g) | SidebarHit::GroupMenu(g)) => self.group_menu(g),
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn project_menu(&self, p: usize) -> Vec<MenuAction> {
        let id = self.projects[p].id;
        let mut actions = vec![MenuAction::Rename(Target::Project(id))];
        if !self.groups.is_empty() {
            actions.push(MenuAction::MoveToGroup(id));
        }
        if git::is_repo_root(&self.projects[p].path) {
            actions.push(MenuAction::ProjectSettings(id));
            actions.push(MenuAction::Worktrees(id));
        }
        actions.push(self.open_entry(super::opening::Subject::Project(id)));
        actions.extend(self.plugin_menu(Spot::Project(id)));
        actions
    }

    fn group_menu(&self, g: usize) -> Vec<MenuAction> {
        let id = self.groups[g].id;
        vec![
            MenuAction::AddProject(id),
            MenuAction::Rename(Target::Group(id)),
            MenuAction::GroupStyle(id),
            MenuAction::DeleteGroup(id),
        ]
    }

    pub(super) fn open_tree_menu(&mut self, list: Rect, pos: Position) {
        if self.remote_tree_menu(list, pos) {
            return;
        }
        let Some(ui::TreeHit::Fold(row) | ui::TreeHit::Select(row) | ui::TreeHit::Close(row) | ui::TreeHit::Menu(row)) =
            self.tree_hit(list, &self.tree_shape(), pos)
        else {
            return;
        };
        let actions = match (row, self.tree_target(row)) {
            (ui::TreeRow::Group(g), _) => self.group_menu(g),
            (ui::TreeRow::Project(p), Some(_)) => self.project_menu(p),
            (_, Some(target)) => self.target_menu(target),
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    pub(super) fn open_workspace_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        if self.remote_row_menu(list, pitch, pos) || self.project().is_none() {
            return;
        }
        let hit = self.workspace_hit(list, pitch, &self.tab_lines(), pos);
        let Some(target) = hit.and_then(|hit| self.workspace_hit_target(self.active, hit)) else { return };
        self.overlay = Some(Overlay::Menu { at: pos, actions: self.target_menu(target) });
    }

    fn workspace_hit(&self, list: Rect, pitch: u16, tabs: &ui::TabLines, pos: Position) -> Option<WorkspaceHit> {
        let ours = |w: usize| self.projects.workspace_at(self.active, w).is_some() && !self.removing(self.active, w);
        let alone = self.remote_alone();
        ui::workspace_hit(list, pitch, tabs, self.scroll.of(List::Workspaces), pos)
            .filter(|hit| !alone && hit.workspace().is_none_or(ours))
    }

    fn sidebar_hit(&self, list: Rect, pitch: u16, rows: &[SidebarRow], pos: Position) -> Option<SidebarHit> {
        let hit = ui::sidebar_hit(list, pitch, rows, self.scroll.of(List::Projects), pos)?;
        let (SidebarHit::Select(p) | SidebarHit::Close(p) | SidebarHit::Menu(p)) = hit else { return Some(hit) };
        (p < self.projects.len()).then_some(hit)
    }
}
