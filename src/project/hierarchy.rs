use std::path::Path;

use super::{Group, Project, Tab, Workspace};
use crate::term::Term;

pub trait Hierarchy {
    fn project_index(&self, id: u64) -> Option<usize>;
    fn workspace_index(&self, id: u64) -> Option<(usize, usize)>;
    fn workspace_in(&self, project: u64, workspace: u64) -> Option<(usize, usize)>;
    fn tab_index(&self, id: u64) -> Option<(usize, usize, usize)>;
    fn tab_in(&self, project: u64, workspace: u64, tab: u64) -> Option<(usize, usize, usize)>;
    fn tab_of_pane(&self, pane: u64) -> Option<(usize, usize, usize)>;
    fn pane_by(&self, id: u64) -> Option<&Term>;
    fn pane_by_mut(&mut self, id: u64) -> Option<&mut Term>;
    fn tab_with_pane(&self, pane: u64) -> Option<(&Path, &Tab)>;
    fn tab_with_pane_mut(&mut self, pane: u64) -> Option<(&Path, &mut Tab)>;
    fn workspaces(&self) -> impl Iterator<Item = &Workspace>;
    fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace>;
    fn tabs(&self) -> impl Iterator<Item = &Tab>;
    fn tabs_mut(&mut self) -> impl Iterator<Item = &mut Tab>;
    fn terms(&self) -> impl Iterator<Item = &Term>;
    fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term>;
}

impl Hierarchy for [Project] {
    fn project_index(&self, id: u64) -> Option<usize> {
        self.iter().position(|p| p.id == id)
    }

    fn workspace_index(&self, id: u64) -> Option<(usize, usize)> {
        self.iter().enumerate().find_map(|(p, project)| project.index_of(id).map(|w| (p, w)))
    }

    fn workspace_in(&self, project: u64, workspace: u64) -> Option<(usize, usize)> {
        let p = self.project_index(project)?;
        let w = self[p].index_of(workspace)?;
        Some((p, w))
    }

    fn tab_index(&self, id: u64) -> Option<(usize, usize, usize)> {
        find_tab(self, |_, _, tab| tab.id == id)
    }

    fn tab_in(&self, project: u64, workspace: u64, tab: u64) -> Option<(usize, usize, usize)> {
        find_tab(self, |p, w, t| p.id == project && w.id == workspace && t.id == tab)
    }

    fn tab_of_pane(&self, pane: u64) -> Option<(usize, usize, usize)> {
        find_tab(self, |_, _, tab| tab.index_of(pane).is_some())
    }

    fn pane_by(&self, id: u64) -> Option<&Term> {
        self.tab_with_pane(id)?.1.pane_by(id)
    }

    fn pane_by_mut(&mut self, id: u64) -> Option<&mut Term> {
        self.tab_with_pane_mut(id)?.1.pane_by_mut(id)
    }

    fn tab_with_pane(&self, pane: u64) -> Option<(&Path, &Tab)> {
        let (p, w, t) = self.tab_of_pane(pane)?;
        let workspace = &self[p].workspaces[w];
        Some((&workspace.path, &workspace.tabs[t]))
    }

    fn tab_with_pane_mut(&mut self, pane: u64) -> Option<(&Path, &mut Tab)> {
        let (p, w, t) = self.tab_of_pane(pane)?;
        let workspace = &mut self[p].workspaces[w];
        Some((&workspace.path, &mut workspace.tabs[t]))
    }

    fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.iter().flat_map(|p| &p.workspaces)
    }

    fn workspaces_mut(&mut self) -> impl Iterator<Item = &mut Workspace> {
        self.iter_mut().flat_map(|p| &mut p.workspaces)
    }

    fn tabs(&self) -> impl Iterator<Item = &Tab> {
        self.workspaces().flat_map(|w| &w.tabs)
    }

    fn tabs_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.workspaces_mut().flat_map(|w| &mut w.tabs)
    }

    fn terms(&self) -> impl Iterator<Item = &Term> {
        self.tabs().flat_map(|t| &t.panes)
    }

    fn terms_mut(&mut self) -> impl Iterator<Item = &mut Term> {
        self.tabs_mut().flat_map(|t| &mut t.panes)
    }
}

fn find_tab(projects: &[Project], found: impl Fn(&Project, &Workspace, &Tab) -> bool) -> Option<(usize, usize, usize)> {
    projects.iter().enumerate().find_map(|(p, project)| {
        project.workspaces.iter().enumerate().find_map(|(w, workspace)| {
            workspace.tabs.iter().position(|tab| found(project, workspace, tab)).map(|t| (p, w, t))
        })
    })
}

pub fn group_indices(projects: &[Project], groups: &[Group]) -> Vec<Option<usize>> {
    projects.iter().map(|p| p.group.and_then(|id| groups.iter().position(|g| g.id == id))).collect()
}

pub fn sidebar_order(projects: &[Project], groups: &[Group]) -> Vec<usize> {
    let indices = group_indices(projects, groups);
    let in_group = |group| indices.iter().enumerate().filter_map(move |(p, g)| (*g == group).then_some(p));
    let mut order: Vec<_> = in_group(None).collect();
    for g in 0..groups.len() {
        order.extend(in_group(Some(g)));
    }
    order
}

#[cfg(test)]
mod tests;
