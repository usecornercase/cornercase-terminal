use ratatui::layout::{Position, Rect};

use super::RemoteItem;
use super::column::Column;
use super::rows::{RemoteProject, unpaired};
use crate::app::App;
use crate::app::interaction::Grab;
use crate::app::layout::List;
use crate::ui::{self, SidebarHit, SidebarRow, TreeHit, TreeRow, WorkspaceHit, WorkspaceRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum RemoteAt {
    Project { link: u64, id: u64 },
    Workspace { link: u64, id: u64 },
    Tab { link: u64, workspace: u64, id: u64 },
}

impl RemoteAt {
    fn link(self) -> u64 {
        match self {
            Self::Project { link, .. } | Self::Workspace { link, .. } | Self::Tab { link, .. } => link,
        }
    }

    fn workspace(self) -> Option<u64> {
        match self {
            Self::Workspace { id, .. } | Self::Tab { workspace: id, .. } => Some(id),
            Self::Project { .. } => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Button {
    Close,
    Menu,
}

fn row_of(hit: WorkspaceHit) -> Option<(WorkspaceRow, bool)> {
    Some(match hit {
        WorkspaceHit::Fold(w) => (WorkspaceRow::Workspace(w), true),
        WorkspaceHit::Workspace(w) | WorkspaceHit::CloseWorkspace(w) | WorkspaceHit::WorkspaceMenu(w) => {
            (WorkspaceRow::Workspace(w), false)
        }
        WorkspaceHit::Tab(w, t) | WorkspaceHit::CloseTab(w, t) | WorkspaceHit::TabMenu(w, t) => {
            (WorkspaceRow::Tab(w, t), false)
        }
        _ => return None,
    })
}

fn row_button(hit: WorkspaceHit) -> Option<Button> {
    match hit {
        WorkspaceHit::CloseWorkspace(_) => Some(Button::Close),
        WorkspaceHit::WorkspaceMenu(_) => Some(Button::Menu),
        _ => None,
    }
}

fn tree_button(hit: TreeHit) -> Option<Button> {
    match hit {
        TreeHit::Close(_) => Some(Button::Close),
        TreeHit::Menu(_) => Some(Button::Menu),
        _ => None,
    }
}

fn remote_row(column: &Column, row: WorkspaceRow) -> Option<RemoteAt> {
    match row {
        WorkspaceRow::Workspace(w) => column.remote(w).map(|(r, w)| RemoteAt::Workspace { link: r.link, id: w.id }),
        WorkspaceRow::Tab(w, t) => {
            let (r, w) = column.remote(w)?;
            Some(RemoteAt::Tab { link: r.link, workspace: w.id, id: w.tabs.get(t)?.id })
        }
        _ => None,
    }
}

impl App {
    pub(in crate::app) fn press_remote_row(
        &mut self,
        list: Rect,
        pitch: u16,
        tabs: &ui::TabLines,
        (pos, area): (Position, Rect),
    ) -> bool {
        let scroll = self.scroll.of(List::Workspaces);
        let Some((hit, at)) = self.remote_hit(list, pitch, tabs, pos) else { return false };
        if self.press_remote_button(at, row_button(hit), pos) {
            return true;
        }
        let Some((row, fold)) = row_of(hit) else { return false };
        self.grab_with(Grab::Remote(at), ui::workspace_row(list, pitch, tabs, scroll, row), area, fold);
        true
    }

    fn remote_hit(
        &self,
        list: Rect,
        pitch: u16,
        tabs: &ui::TabLines,
        pos: Position,
    ) -> Option<(WorkspaceHit, RemoteAt)> {
        let hit = ui::workspace_hit(list, pitch, tabs, self.scroll.of(List::Workspaces), pos)?;
        let (row, _) = row_of(hit)?;
        let remote = self.remote_projects();
        let at = remote_row(&self.column(&remote), row)?;
        Some((hit, at))
    }

    pub(in crate::app) fn remote_row_menu(&mut self, list: Rect, pitch: u16, pos: Position) -> bool {
        let tabs = self.tab_lines();
        let Some((_, at)) = self.remote_hit(list, pitch, &tabs, pos) else { return false };
        self.press_remote_button(at, Some(Button::Menu), pos)
    }

    fn press_remote_button(&mut self, at: RemoteAt, button: Option<Button>, pos: Position) -> bool {
        if at.workspace().is_some_and(|id| self.machines.acting.removing(at.link(), id)) {
            return true;
        }
        let (Some(button), RemoteAt::Workspace { link, id }) = (button, at) else { return false };
        let Some(aim) = self.aim_at(link, id) else { return true };
        match button {
            Button::Close => self.remote_close(aim),
            Button::Menu => self.remote_menu(aim, pos),
        }
        true
    }

    pub(in crate::app) fn press_remote_tree(
        &mut self,
        list: Rect,
        shape: &ui::TreeShape,
        hit: Option<TreeHit>,
        (pos, area): (Position, Rect),
    ) -> bool {
        let Some(hit @ (TreeHit::Select(row) | TreeHit::Fold(row) | TreeHit::Close(row) | TreeHit::Menu(row))) = hit
        else {
            return false;
        };
        let Some(at) = self.remote_tree_at(row) else { return false };
        if self.press_remote_button(at, tree_button(hit), pos) {
            return true;
        }
        let rect = ui::tree_row(list, shape, self.scroll.of(List::Projects), row);
        self.grab_with(Grab::Remote(at), rect, area, matches!(hit, TreeHit::Fold(_)));
        true
    }

    pub(in crate::app) fn remote_tree_menu(&mut self, list: Rect, pos: Position) -> bool {
        let shape = self.tree_shape();
        let Some(TreeHit::Select(row) | TreeHit::Fold(row) | TreeHit::Close(row) | TreeHit::Menu(row)) =
            self.tree_hit(list, &shape, pos)
        else {
            return false;
        };
        let at = self.remote_tree_at(row);
        at.is_some_and(|at| self.press_remote_button(at, Some(Button::Menu), pos))
    }

    fn remote_tree_at(&self, row: TreeRow) -> Option<RemoteAt> {
        let remote = self.remote_projects();
        match row {
            TreeRow::Project(p) => self.remote_project_at(&remote, p),
            TreeRow::Workspace(p, w) => remote_row(&self.column_at(&remote, p), WorkspaceRow::Workspace(w)),
            TreeRow::Tab(p, w, t) => remote_row(&self.column_at(&remote, p), WorkspaceRow::Tab(w, t)),
            _ => None,
        }
    }

    fn remote_project_at(&self, remote: &[RemoteProject], p: usize) -> Option<RemoteAt> {
        let only = unpaired(remote).nth(p.checked_sub(self.projects.len())?)?;
        Some(RemoteAt::Project { link: only.link, id: only.project.id })
    }

    pub(in crate::app) fn press_remote_project(
        &mut self,
        list: Rect,
        pitch: u16,
        rows: &[SidebarRow],
        (pos, area): (Position, Rect),
    ) -> bool {
        let scroll = self.scroll.of(List::Projects);
        let hit = ui::sidebar_hit(list, pitch, rows, scroll, pos);
        let Some(SidebarHit::Select(p) | SidebarHit::Close(p) | SidebarHit::Menu(p)) = hit else { return false };
        let remote = self.remote_projects();
        let Some(at) = self.remote_project_at(&remote, p) else { return false };
        drop(remote);
        self.grab_with(Grab::Remote(at), ui::entry_row(list, pitch, rows, scroll, SidebarRow::Project(p)), area, false);
        true
    }

    pub(in crate::app) fn click_remote(&mut self, at: RemoteAt) {
        let Some((machine, item)) = self.remote_item(at) else { return };
        if self.select_remote(&machine, item) {
            let project = matches!(at, RemoteAt::Project { .. });
            self.screen.nav = self.screen.nav.filter(|_| project).map(|_| ui::Nav::Workspaces);
        }
    }

    fn remote_item(&self, at: RemoteAt) -> Option<(String, RemoteItem)> {
        let held = self.machines.held.iter().find(|held| held.link.id() == at.link())?;
        let item = match at {
            RemoteAt::Project { id, .. } => {
                let seen = held.seen.as_deref()?;
                let project = seen.projects.iter().find(|project| project.id == id)?;
                RemoteItem::Workspace(project.workspaces.first()?.id)
            }
            RemoteAt::Workspace { id, .. } => RemoteItem::Workspace(id),
            RemoteAt::Tab { id, .. } => RemoteItem::Tab(id),
        };
        Some((held.name.clone(), item))
    }

    pub(in crate::app) fn fold_remote(&mut self, at: RemoteAt) {
        let RemoteAt::Workspace { link, id } = at else { return };
        let remote = self.remote_projects();
        let theirs = remote.iter().filter(|r| r.link == link);
        let mut workspaces = theirs.flat_map(|r| r.project.workspaces.iter().map(move |w| (r, w)));
        let key = workspaces.find(|(_, w)| w.id == id).map(|(r, w)| (r.tag.name.clone(), w.path.clone()));
        drop(remote);
        let Some(key) = key else { return };
        let folded = &mut self.machines.folded;
        let before = folded.len();
        folded.retain(|kept| *kept != key);
        if folded.len() == before {
            folded.push(key);
        }
    }
}
