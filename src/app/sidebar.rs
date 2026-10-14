use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::Drag;
use super::interaction::Grab;
use super::interaction::RowDrag;
use super::menus::MenuAction;
use super::menus::Target;
use super::overlay::Overlay;
use crate::agents::Known;
use crate::error::Result;
use crate::git;
use crate::project::Workspace;
use crate::ui::{self, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};

mod drag;
mod tree;

impl App {
    pub(super) fn sidebar_rows(&self) -> Vec<SidebarRow> {
        let collapsed: Vec<bool> = self.groups.iter().map(|g| g.entry.collapsed).collect();
        ui::sidebar_rows(&crate::project::group_indices(&self.projects, &self.groups), &collapsed)
    }

    pub(super) fn active_row(&self, sidebar: &[SidebarRow]) -> Option<usize> {
        let group = self.project().and_then(|p| p.group).and_then(|id| self.group_index(id));
        ui::active_row(sidebar, self.active, group)
    }

    pub(super) fn click_projects(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) {
        let rows = self.sidebar_rows();
        let rect = |row: SidebarRow| ui::entry_row(list, pitch, &rows, self.scroll.projects, row);
        match ui::sidebar_hit(list, pitch, &rows, self.scroll.projects, pos) {
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

    pub(super) fn sidebar_layout(&self, areas: &ui::Areas) -> ui::Rows {
        if areas.tree {
            self.tree_layout(areas.list)
        } else {
            ui::project_rows(areas.list, areas.pitch, &self.sidebar_rows(), self.scroll.projects)
        }
    }

    pub(super) fn close_row(&mut self, row: ui::TreeRow) {
        match row {
            ui::TreeRow::Group(g) => self.overlay = Some(Overlay::DeleteGroup { group: self.groups[g].id }),
            ui::TreeRow::Project(p) => self.overlay = Some(Overlay::CloseProject { project: self.projects[p].id }),
            ui::TreeRow::Workspace(p, w) => self.close_workspace(p, w),
            ui::TreeRow::Tab(p, w, t) => self.close_tab(p, w, t),
            _ => {}
        }
    }

    pub(super) fn ask_new_workspace(&mut self, p: usize) {
        let project = &self.projects[p];
        let worktree = git::is_repo_root(&project.path).then_some(true);
        self.overlay = Some(Overlay::NewWorkspace {
            project: project.id,
            input: String::new(),
            worktree,
            error: None,
            creating: false,
        });
    }

    pub(super) fn agents_view(&self) -> ui::AgentsView {
        let focus = self.focus();
        let entries = self.agent_places().into_iter().map(|(p, w, t, i)| {
            let project = &self.projects[p];
            let workspace = &project.workspaces[w];
            let tab = &workspace.tabs[t];
            let term = &tab.panes[i];
            let context = term.context.context();
            ui::AgentEntry {
                status: term.agent.status(),
                agent: term.agent.agent().map(Known::name).unwrap_or_default().to_string(),
                project: self.project_label(project),
                workspace: (project.workspaces.len() > 1).then(|| workspace.label()),
                details: self.details(context, None),
                active: focus.tab == Some(tab.id) && tab.active == i,
            }
        });
        ui::AgentsView { entries: entries.collect(), scroll: self.scroll.agents }
    }

    pub(super) fn agent_layout(&self, areas: &ui::Areas) -> ui::Rows {
        ui::agent_rows(areas.agents_list, areas.pitch, self.agent_places().len(), self.scroll.agents)
    }

    pub(super) fn press_agent(&mut self, areas: &ui::Areas, pos: Position, area: Rect) {
        let places = self.agent_places();
        let (list, pitch, scroll) = (areas.agents_list, areas.pitch, self.scroll.agents);
        let Some(i) = ui::agent_hit(list, pitch, places.len(), scroll, pos) else { return };
        let (p, w, t, pane) = places[i];
        let pane = self.projects[p].workspaces[w].tabs[t].panes[pane].id;
        let row = ui::agent_row(list, pitch, places.len(), scroll, i);
        self.drag = Some(Drag::Row(RowDrag {
            target: Grab::Agent(pane),
            row,
            moved: false,
            area,
            scrolled: None,
            fold: false,
        }));
    }

    pub(super) fn tab_lines(&self) -> ui::TabLines {
        let lines = |w: &Workspace| -> Vec<u16> { w.tabs.iter().map(|t| self.tab_details(t).lines()).collect() };
        let lines = self.project().map(|p| p.workspaces.iter().map(lines).collect()).unwrap_or_default();
        ui::TabLines::new(lines, self.drawn.tab_bar.is_empty())
    }

    pub(super) fn click_workspaces(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) -> Result<()> {
        if self.project().is_none() {
            return Ok(());
        }
        let tabs = self.tab_lines();
        let hit = self.workspace_hit(list, pitch, &tabs, pos);
        if matches!(hit, Some(WorkspaceHit::NewTab(_) | WorkspaceHit::NewWorkspace)) {
            self.nav = None;
        }
        let p = self.active;
        let ask = pitch > 1;
        let rect = |row: WorkspaceRow| ui::workspace_row(list, pitch, &tabs, self.scroll.workspaces, row);
        let project = &self.projects[p];
        match hit {
            Some(WorkspaceHit::Workspace(w)) => {
                let target = Target::Workspace(project.id, project.workspaces[w].id);
                self.grab(target, rect(WorkspaceRow::Workspace(w)), area, false);
            }
            Some(WorkspaceHit::CloseWorkspace(w)) if ask && !project.workspaces[w].worktree => {
                self.overlay =
                    Some(Overlay::CloseWorkspace { project: project.id, workspace: project.workspaces[w].id });
            }
            Some(WorkspaceHit::CloseWorkspace(w)) => self.close_workspace(p, w),
            Some(WorkspaceHit::Tab(w, t)) => {
                let workspace = &project.workspaces[w];
                let target = Target::Tab(project.id, workspace.id, workspace.tabs[t].id);
                self.grab(target, rect(WorkspaceRow::Tab(w, t)), area, false);
            }
            Some(WorkspaceHit::CloseTab(w, t)) if ask => {
                let workspace = &project.workspaces[w];
                let (project, workspace, tab) = (project.id, workspace.id, workspace.tabs[t].id);
                self.overlay = Some(Overlay::CloseTab { project, workspace, tab });
            }
            Some(WorkspaceHit::CloseTab(w, t)) => self.close_tab(p, w, t),
            Some(WorkspaceHit::NewTab(w)) => self.add_tab(p, w, area)?,
            Some(WorkspaceHit::NewWorkspace) => self.ask_new_workspace(p),
            Some(WorkspaceHit::WorkspaceMenu(_) | WorkspaceHit::TabMenu(..)) => {
                self.open_workspace_menu(list, pitch, pos);
            }
            None => {}
        }
        Ok(())
    }

    pub(super) fn open_project_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let actions = match ui::sidebar_hit(list, pitch, &self.sidebar_rows(), self.scroll.projects, pos) {
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
        let Some(ui::TreeHit::Fold(row) | ui::TreeHit::Select(row) | ui::TreeHit::Close(row) | ui::TreeHit::Menu(row)) =
            self.tree_hit(list, &self.tree_shape(), pos)
        else {
            return;
        };
        let actions = match (row, self.tree_target(row)) {
            (ui::TreeRow::Group(g), _) => self.group_menu(g),
            (ui::TreeRow::Project(p), _) => self.project_menu(p),
            (_, Some(target)) => self.target_menu(target),
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    pub(super) fn open_workspace_menu(&mut self, list: Rect, pitch: u16, pos: Position) {
        let Some(project) = self.project() else { return };
        let target = match self.workspace_hit(list, pitch, &self.tab_lines(), pos) {
            Some(WorkspaceHit::Workspace(w) | WorkspaceHit::CloseWorkspace(w) | WorkspaceHit::WorkspaceMenu(w)) => {
                Target::Workspace(project.id, project.workspaces[w].id)
            }
            Some(WorkspaceHit::Tab(w, t) | WorkspaceHit::CloseTab(w, t) | WorkspaceHit::TabMenu(w, t)) => {
                let workspace = &project.workspaces[w];
                Target::Tab(project.id, workspace.id, workspace.tabs[t].id)
            }
            _ => return,
        };
        self.overlay = Some(Overlay::Menu { at: pos, actions: self.target_menu(target) });
    }

    fn workspace_hit(&self, list: Rect, pitch: u16, tabs: &ui::TabLines, pos: Position) -> Option<WorkspaceHit> {
        ui::workspace_hit(list, pitch, tabs, self.scroll.workspaces, pos)
            .filter(|hit| !hit.workspace().is_some_and(|w| self.removing(self.active, w)))
    }
}
