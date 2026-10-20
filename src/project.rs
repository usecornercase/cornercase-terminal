use std::path::{Path, PathBuf};

use ratatui::layout::{Position, Rect};

use self::split::{Dir, Node, Place};
use crate::agents::activity::Status;
use crate::agents::context::Context;
use crate::config::Config;
use crate::flows::run::RoleWorktree;
use crate::git::worktree::setup;
use crate::term::Term;

pub mod group;
mod hierarchy;
pub(crate) mod picker;
pub(crate) mod search;
pub(crate) mod settings;
pub mod split;
pub(crate) mod view;
pub(crate) mod vscode;

pub(crate) use group::Group;
pub(crate) use hierarchy::{Hierarchy, TabAt, WorkspaceAt, group_indices, sidebar_order};

pub(crate) struct Landing {
    pub(crate) area: Rect,
    pub(crate) place: Place,
    pub(crate) layout: Node<u64>,
}

pub(crate) struct Tab {
    pub(crate) id: u64,
    pub(crate) name: Option<String>,
    pub(crate) panes: Vec<Term>,
    pub(crate) active: usize,
    pub(crate) layout: Node<u64>,
    pub(crate) right_clicks: Vec<u64>,
}

impl Tab {
    pub(crate) fn new(id: u64, name: Option<String>, term: Term) -> Self {
        let layout = Node::Leaf(term.id);
        Self { id, name, panes: vec![term], active: 0, layout, right_clicks: Vec::new() }
    }

    pub(crate) fn restored(id: u64, name: Option<String>, panes: Vec<Term>, layout: Option<&Node<usize>>) -> Self {
        let ids: Vec<u64> = panes.iter().map(|t| t.id).collect();
        let mapped = layout.and_then(|l| l.map(&|i| ids.get(i).copied()));
        let layout = match mapped {
            Some(l) if is_permutation(&l.ids(), &ids) => l,
            _ => split::row(&ids).unwrap_or(Node::Leaf(0)),
        };
        Self { id, name, panes, active: 0, layout, right_clicks: Vec::new() }
    }

    pub(crate) fn index_of(&self, pane: u64) -> Option<usize> {
        self.panes.iter().position(|t| t.id == pane)
    }

    pub(crate) fn pane_by(&self, id: u64) -> Option<&Term> {
        self.panes.get(self.index_of(id)?)
    }

    pub(crate) fn pane_by_mut(&mut self, id: u64) -> Option<&mut Term> {
        let i = self.index_of(id)?;
        self.panes.get_mut(i)
    }

    pub(crate) fn layout_by_index(&self) -> Option<Node<usize>> {
        self.layout.map(&|id| self.index_of(id))
    }

    pub(crate) fn pane(&self) -> Option<&Term> {
        self.panes.get(self.active)
    }

    pub(crate) fn pane_mut(&mut self) -> Option<&mut Term> {
        self.panes.get_mut(self.active)
    }

    pub(crate) fn label(&self, config: &Config) -> String {
        self.name.clone().or_else(|| self.first().and_then(|t| t.program(config))).unwrap_or_else(|| "?".into())
    }

    fn first(&self) -> Option<&Term> {
        let first = self.layout.first();
        self.panes.iter().find(|t| t.id == first).or_else(|| self.panes.first())
    }

    pub(crate) fn others(&self) -> usize {
        self.panes.len().saturating_sub(1)
    }

    pub(crate) fn status(&self) -> Option<Status> {
        self.panes
            .iter()
            .filter_map(|t| t.agent.status().or_else(|| t.setup.as_ref().and_then(setup::Pane::status)))
            .max()
    }

    pub(crate) fn context(&self) -> Option<&Context> {
        self.pane().and_then(|t| t.context.context()).or_else(|| self.panes.iter().find_map(|t| t.context.context()))
    }

    pub(crate) fn running(&self) -> bool {
        self.panes.iter().any(|t| !t.shell_in_foreground())
    }

    pub(crate) fn memory(&self) -> Option<u64> {
        self.panes.iter().filter_map(|t| t.memory.bytes()).reduce(u64::saturating_add)
    }

    pub(crate) fn subagents(&self) -> usize {
        self.panes.iter().map(|t| t.context.subagents()).sum()
    }

    pub(crate) fn focus(&mut self, id: u64) {
        if let Some(i) = self.panes.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    pub(crate) fn shown(&self, area: Rect) -> Vec<(u64, Rect)> {
        self.pane().map(|t| self.layout.visible(area, t.id)).unwrap_or_default()
    }

    pub(crate) fn rect(&self, area: Rect, id: u64) -> Option<Rect> {
        self.layout.pane(area, self.pane()?.id, id)
    }

    pub(crate) fn pane_at(&self, area: Rect, pos: Position) -> Option<u64> {
        self.layout.pane_at(area, self.pane()?.id, pos)
    }

    pub(crate) fn landing(&self, area: Rect, pane: u64, pos: Position) -> Option<Landing> {
        let target = self.pane_at(area, pos).filter(|t| *t != pane)?;
        let rect = self.rect(area, target)?;
        let place = Place::at(rect, pos);
        let layout = self.rearranged(area, pane, target, place)?;
        Some(Landing { area: place.area(rect), place, layout })
    }

    pub(crate) fn rearranged(&self, area: Rect, pane: u64, target: u64, place: Place) -> Option<Node<u64>> {
        self.layout.moved(pane, target, place).filter(|l| l.has_room(area))
    }

    pub(crate) fn land(&mut self, pane: u64, layout: Node<u64>) {
        self.layout = layout;
        self.focus(pane);
    }

    pub(crate) fn can_split(&self, area: Rect, id: u64, dir: Dir) -> bool {
        self.layout.has_room(area) && self.rect(area, id).is_some_and(|r| split::fits(r, dir))
    }

    pub(crate) fn split(&mut self, target: u64, dir: Dir, term: Term, focus: bool) {
        if self.layout.split(target, dir, term.id) {
            self.panes.push(term);
            if focus {
                self.active = self.panes.len() - 1;
            }
        }
    }

    pub(crate) fn right_clicks_to_pane(&self, id: u64) -> bool {
        self.right_clicks.contains(&id)
    }

    pub(crate) fn toggle_right_clicks(&mut self, id: u64) {
        if let Some(i) = self.right_clicks.iter().position(|p| *p == id) {
            self.right_clicks.remove(i);
        } else {
            self.right_clicks.push(id);
        }
    }

    fn remove(&mut self, id: u64) -> bool {
        let Some(p) = self.panes.iter().position(|t| t.id == id) else { return false };
        let near = self.layout.remove(id);
        self.panes.remove(p);
        self.right_clicks.retain(|r| *r != id);
        let near = near.and_then(|n| self.panes.iter().position(|t| t.id == n));
        match near {
            Some(n) if p == self.active => self.active = n,
            _ => shift_active(&mut self.active, p),
        }
        true
    }
}

fn is_permutation(found: &[u64], expected: &[u64]) -> bool {
    let mut found = found.to_vec();
    let mut expected = expected.to_vec();
    found.sort_unstable();
    expected.sort_unstable();
    found == expected
}

pub(crate) struct Workspace {
    pub(crate) id: u64,
    pub(crate) path: PathBuf,
    pub(crate) name: Option<String>,
    pub(crate) worktree: bool,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    pub(crate) phase: Phase,
    pub(crate) behind: u32,
    pub(crate) base: Option<String>,
    pub(crate) collapsed: bool,
    pub(crate) role_worktrees: Vec<RoleWorktree>,
    pub(crate) run_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Phase {
    Open,
    Removing(String),
    Closing,
}

impl Workspace {
    pub(crate) fn new(id: u64, path: PathBuf, name: Option<String>, worktree: bool) -> Self {
        Self {
            id,
            path,
            name,
            worktree,
            tabs: Vec::new(),
            active: 0,
            phase: Phase::Open,
            behind: 0,
            base: None,
            collapsed: false,
            role_worktrees: Vec::new(),
            run_dirs: Vec::new(),
        }
    }

    pub(crate) fn setup(&self) -> Option<(u64, &setup::Pane)> {
        let panes = || self.tabs.iter().flat_map(|t| &t.panes).filter_map(|t| t.setup.as_ref().map(|s| (t.id, s)));
        panes().find(|(_, s)| s.running()).or_else(|| panes().next())
    }

    pub(crate) fn stop_blaming_setup(&mut self) {
        for term in self.tabs.iter_mut().flat_map(|t| &mut t.panes) {
            if let Some(setup::Pane::Failed { blame, .. }) = &mut term.setup {
                *blame = false;
            }
        }
    }

    pub(crate) fn clear_failed_setups(&mut self) {
        for term in self.tabs.iter_mut().flat_map(|t| &mut t.panes) {
            if matches!(term.setup, Some(setup::Pane::Failed { .. })) {
                term.setup = None;
            }
        }
    }

    pub(crate) fn setting_up(&self) -> bool {
        self.setup().is_some_and(|(_, s)| s.running())
    }

    pub(crate) fn open(&self) -> bool {
        self.phase == Phase::Open
    }

    pub(crate) fn closing(&self) -> bool {
        self.phase == Phase::Closing
    }

    pub(crate) fn removing(&self) -> bool {
        matches!(self.phase, Phase::Removing(_))
    }

    pub(crate) fn start_removing(&mut self) {
        self.phase = Phase::Removing(self.label());
    }

    pub(crate) fn index_of(&self, tab: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab)
    }

    pub(crate) fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub(crate) fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub(crate) fn terms(&self) -> impl Iterator<Item = &Term> {
        self.tabs.iter().flat_map(|t| &t.panes)
    }

    pub(crate) fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term> {
        self.tabs.iter_mut().flat_map(|t| &mut t.panes)
    }

    pub(crate) fn label(&self) -> String {
        if let Phase::Removing(label) = &self.phase {
            return label.clone();
        }
        self.name.clone().or_else(|| crate::git::branch(&self.path)).unwrap_or_else(|| "default".into())
    }

    pub(crate) fn remove_term(&mut self, id: u64) -> bool {
        let Some(t) = self.tabs.iter().position(|tab| tab.panes.iter().any(|p| p.id == id)) else { return false };
        let tab = &mut self.tabs[t];
        tab.remove(id);
        if tab.panes.is_empty() {
            self.tabs.remove(t);
            shift_active(&mut self.active, t);
        }
        true
    }

    pub(crate) fn kill(&mut self) {
        for term in self.terms_mut() {
            term.kill();
        }
    }
}

pub(crate) struct Project {
    pub(crate) id: u64,
    pub(crate) path: PathBuf,
    pub(crate) name: Option<String>,
    pub(crate) group: Option<u64>,
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) active: usize,
    pub(crate) closing: bool,
    pub(crate) collapsed: bool,
}

impl Project {
    pub(crate) fn new(id: u64, path: PathBuf, name: Option<String>) -> Self {
        Self { id, path, name, group: None, workspaces: Vec::new(), active: 0, closing: false, collapsed: false }
    }

    pub(crate) fn index_of(&self, workspace: u64) -> Option<usize> {
        self.workspaces.iter().position(|w| w.id == workspace)
    }

    pub(crate) fn label(&self, home: Option<&Path>) -> String {
        self.name.clone().unwrap_or_else(|| crate::text::folder_name(&self.path, home))
    }

    pub(crate) fn workspace(&self) -> Option<&Workspace> {
        self.workspaces.get(self.active)
    }

    pub(crate) fn workspace_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces.get_mut(self.active)
    }

    pub(crate) fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term> {
        self.workspaces.iter_mut().flat_map(Workspace::terms_mut)
    }

    pub(crate) fn has_terms(&self) -> bool {
        self.workspaces.iter().any(|w| w.terms().next().is_some())
    }

    pub(crate) fn remove_term(&mut self, id: u64) -> bool {
        let Some(w) = self.workspaces.iter_mut().position(|w| w.remove_term(id)) else { return false };
        if self.workspaces[w].closing() && self.workspaces[w].tabs.is_empty() {
            self.remove_workspace(w);
        }
        true
    }

    pub(crate) fn step_off(&mut self, w: usize) {
        if self.active != w {
            return;
        }
        let mut others = (0..w).rev().chain(w + 1..self.workspaces.len());
        if let Some(other) = others.find(|&o| self.workspaces[o].open()) {
            self.active = other;
        }
    }

    pub(crate) fn remove_workspace(&mut self, w: usize) {
        self.workspaces.remove(w);
        shift_active(&mut self.active, w);
    }

    pub(crate) fn kill(&mut self) {
        for term in self.terms_mut() {
            term.kill();
        }
    }
}

pub(crate) fn shift_active(active: &mut usize, removed: usize) {
    if *active >= removed && *active > 0 {
        *active -= 1;
    }
}

pub(crate) fn move_before<T>(items: &mut Vec<T>, from: usize, before: usize, active: Option<&mut usize>) {
    let to = if before > from { before - 1 } else { before };
    if from >= items.len() || to >= items.len() || to == from {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
    if let Some(active) = active {
        *active = match *active {
            a if a == from => to,
            a if from < a && a <= to => a - 1,
            a if to <= a && a < from => a + 1,
            a => a,
        };
    }
}

#[cfg(test)]
mod tests;
