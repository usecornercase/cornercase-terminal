use std::collections::HashSet;

use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::interaction::Drag;
use super::interaction::Grab;
use super::interaction::RowDrag;
use super::layout::List;
use super::menus::Marked;
use super::menus::MenuAction;
use super::menus::Target;
use super::overlay::Overlay;
use super::plugins::Spot;
use crate::agents::activity;
use crate::error::Result;
use crate::flows;
use crate::git;
use crate::project::{Hierarchy, Workspace};
use crate::term::Term;
use crate::ui::{self, SidebarHit, SidebarRow, WorkspaceHit, WorkspaceRow};

mod drag;
mod tree;

struct AgentAt {
    pane: u64,
    row: Rect,
    fold: Option<bool>,
    on_fold: bool,
    on_close: bool,
}

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
        let rect = |row: SidebarRow| ui::entry_row(list, pitch, &rows, self.scroll.of(List::Projects), row);
        match ui::sidebar_hit(list, pitch, &rows, self.scroll.of(List::Projects), pos) {
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
            input: String::new(),
            worktree,
            error: None,
            creating: false,
        });
    }

    fn seated(&self, term: &Term, depth: usize) -> (Option<String>, bool) {
        let Some((run, role)) = self.seat(term.id) else { return (None, true) };
        let under = depth > 0
            && role != flows::ORCHESTRATOR
            && term.parent.and_then(|parent| self.seat(parent)).is_some_and(|(theirs, _)| theirs.id == run.id);
        (Some(role.to_string()), !under)
    }

    pub(super) fn agents_view(&self) -> ui::AgentsView {
        let focus = self.focus();
        let focused = self.term().map(|term| term.id);
        let narrow = self.screen.drawn.agents_list.width < ui::MIN_MENU_ROW_WIDTH;
        let (places, nodes) = self.shown_agents(narrow);
        let entries = nodes.into_iter().map(|node| {
            let (at, i) = places[node.index];
            let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at.workspace()));
            let tab = self.projects.tab(at);
            let term = &tab.panes[i];
            let hidden: Vec<&Term> = node.hidden.iter().map(|&h| self.placed(places[h])).collect();
            let context = term.context.context();
            let (role, place) = self.seated(term, node.depth);
            ui::AgentEntry {
                status: term.agent.status(),
                agent: term.agent.agent().unwrap_or_default().to_string(),
                project: self.project_label(project),
                workspace: (project.workspaces.len() > 1).then(|| workspace.label()),
                role,
                place,
                details: self.details(context, None, term.context.subagents(), self.pane_account(term)),
                active: (focus.tab == Some(tab.id) && tab.active == i)
                    || hidden.iter().any(|term| Some(term.id) == focused),
                depth: node.depth,
                last: node.last,
                rails: node.rails,
                fold: node.has_children.then_some(term.collapsed),
                hidden: hidden.len(),
                attention: activity::attention(hidden.iter().map(|term| term.agent.status())),
            }
        });
        ui::AgentsView { entries: entries.collect(), scroll: self.scroll.of(List::Agents) }
    }

    pub(super) fn agent_layout(&self, areas: &ui::Areas) -> ui::Rows {
        ui::agent_rows(areas.agents_list, areas.pitch, self.shown_agents(false).1.len(), self.scroll.of(List::Agents))
    }

    fn agent_row_at(&self, areas: &ui::Areas, pos: Position) -> Option<AgentAt> {
        let narrow = areas.agents_list.width < ui::MIN_MENU_ROW_WIDTH;
        let (places, nodes) = self.shown_agents(narrow);
        let (list, pitch, scroll) = (areas.agents_list, areas.pitch, self.scroll.of(List::Agents));
        let i = ui::agent_hit(list, pitch, nodes.len(), scroll, pos)?;
        let node = &nodes[i];
        let term = self.placed(places[node.index]);
        let row = ui::agent_row(list, pitch, nodes.len(), scroll, i);
        let fold = node.has_children.then_some(term.collapsed);
        Some(AgentAt {
            pane: term.id,
            row,
            fold,
            on_fold: fold.is_some() && ui::agent_fold(row, node.depth).contains(pos),
            on_close: ui::agent_close(row, pitch).contains(pos),
        })
    }

    pub(super) fn agents_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) {
        match kind {
            _ if !areas.agents_list.contains(pos) => {}
            MouseEventKind::Down(MouseButton::Left) => self.press_agent(areas, pos, area),
            MouseEventKind::Down(MouseButton::Right) => self.open_agent_menu(areas, pos),
            _ => {}
        }
    }

    fn open_agent_menu(&mut self, areas: &ui::Areas, pos: Position) {
        let Some(at) = self.agent_row_at(areas, pos) else { return };
        let actions: Vec<MenuAction> = self
            .mark_action(Marked::Pane(at.pane))
            .into_iter()
            .chain(at.fold.map(|collapsed| MenuAction::AgentFold(at.pane, !collapsed)))
            .chain(std::iter::once(MenuAction::CloseAgent(at.pane)))
            .collect();
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }

    fn press_agent(&mut self, areas: &ui::Areas, pos: Position, area: Rect) {
        let Some(at) = self.agent_row_at(areas, pos) else { return };
        if at.on_close {
            self.ask_close_agent(at.pane);
            return;
        }
        self.screen.drag = Some(Drag::Row(RowDrag {
            target: Grab::Agent(at.pane),
            row: at.row,
            moved: false,
            area,
            scrolled: None,
            fold: at.on_fold,
        }));
    }

    pub(super) fn set_agent_fold(&mut self, pane: u64, collapsed: bool) -> bool {
        let Some(term) = self.projects.pane_by_mut(pane) else { return false };
        term.collapsed = collapsed;
        self.scroll.set(List::Agents, self.agent_layout(&self.screen.drawn).scrolled(0));
        true
    }

    fn toggle_agent_fold(&mut self, pane: u64) {
        if let Some(term) = self.projects.pane_by(pane) {
            self.set_agent_fold(pane, !term.collapsed);
        }
    }

    pub(super) fn reveal_agent(&mut self, pane: u64) -> bool {
        let live = self.pane_ids();
        let mut seen = HashSet::from([pane]);
        let mut at = pane;
        let mut changed = false;
        while let Some(term) = self.projects.pane_by(at) {
            if term.agent.status().is_none() {
                break;
            }
            let Some(parent) = Self::live_parent(term, &live) else { break };
            if !seen.insert(parent) {
                break;
            }
            let Some(ancestor) = self.projects.pane_by_mut(parent) else { break };
            if ancestor.agent.status().is_none() {
                break;
            }
            changed |= std::mem::take(&mut ancestor.collapsed);
            at = parent;
        }
        changed
    }

    pub(super) fn tab_lines(&self) -> ui::TabLines {
        let lines = |w: &Workspace| -> Vec<u16> { w.tabs.iter().map(|t| self.tab_details(t).lines()).collect() };
        let lines = self.project().map(|p| p.workspaces.iter().map(lines).collect()).unwrap_or_default();
        let folded = self.project().map(|p| p.workspaces.iter().map(|w| w.collapsed).collect()).unwrap_or_default();
        ui::TabLines::new(lines, self.screen.drawn.tab_bar.is_empty()).folded(folded)
    }

    pub(super) fn click_workspaces(&mut self, list: Rect, pitch: u16, pos: Position, area: Rect) -> Result<()> {
        if self.project().is_none() {
            return Ok(());
        }
        let tabs = self.tab_lines();
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
        let actions = match ui::sidebar_hit(list, pitch, &self.sidebar_rows(), self.scroll.of(List::Projects), pos) {
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
        if self.project().is_none() {
            return;
        }
        let hit = self.workspace_hit(list, pitch, &self.tab_lines(), pos);
        let Some(target) = hit.and_then(|hit| self.workspace_hit_target(self.active, hit)) else { return };
        self.overlay = Some(Overlay::Menu { at: pos, actions: self.target_menu(target) });
    }

    fn workspace_hit(&self, list: Rect, pitch: u16, tabs: &ui::TabLines, pos: Position) -> Option<WorkspaceHit> {
        ui::workspace_hit(list, pitch, tabs, self.scroll.of(List::Workspaces), pos)
            .filter(|hit| !hit.workspace().is_some_and(|w| self.removing(self.active, w)))
    }
}
