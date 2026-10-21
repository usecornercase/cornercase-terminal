use std::path::Path;

use super::{Group, Project, Tab, Workspace};
use crate::term::Term;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceAt {
    project: usize,
    workspace: usize,
}

impl WorkspaceAt {
    pub(crate) fn project(self) -> usize {
        self.project
    }

    pub(crate) fn index(self) -> usize {
        self.workspace
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TabAt {
    workspace: WorkspaceAt,
    tab: usize,
}

impl TabAt {
    pub(crate) fn project(self) -> usize {
        self.workspace.project
    }

    pub(crate) fn workspace(self) -> WorkspaceAt {
        self.workspace
    }

    pub(crate) fn index(self) -> usize {
        self.tab
    }
}

pub(crate) trait Hierarchy {
    fn project_index(&self, id: u64) -> Option<usize>;
    fn workspace_index(&self, id: u64) -> Option<WorkspaceAt>;
    fn workspace_in(&self, project: u64, workspace: u64) -> Option<WorkspaceAt>;
    fn workspace_at(&self, project: usize, workspace: usize) -> Option<WorkspaceAt>;
    fn active_workspace(&self, project: usize) -> Option<WorkspaceAt>;
    fn find_workspace(&self, project: usize, found: impl Fn(&Workspace) -> bool) -> Option<WorkspaceAt>;
    fn tab_index(&self, id: u64) -> Option<TabAt>;
    fn tab_in(&self, project: u64, workspace: u64, tab: u64) -> Option<TabAt>;
    fn tab_at(&self, project: usize, workspace: usize, tab: usize) -> Option<TabAt>;
    fn active_tab(&self, workspace: WorkspaceAt) -> Option<TabAt>;
    fn tab_of_pane(&self, pane: u64) -> Option<TabAt>;
    fn push_workspace(&mut self, project: usize, workspace: Workspace) -> WorkspaceAt;
    fn push_tab(&mut self, workspace: WorkspaceAt, tab: Tab) -> TabAt;
    fn workspace(&self, at: WorkspaceAt) -> &Workspace;
    fn workspace_mut(&mut self, at: WorkspaceAt) -> &mut Workspace;
    fn workspace_by(&self, id: u64) -> Option<&Workspace>;
    fn workspace_by_mut(&mut self, id: u64) -> Option<&mut Workspace>;
    fn tab(&self, at: TabAt) -> &Tab;
    fn tab_mut(&mut self, at: TabAt) -> &mut Tab;
    fn project_tabs(&self, project: usize) -> impl Iterator<Item = (TabAt, &Tab)>;
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

    fn workspace_index(&self, id: u64) -> Option<WorkspaceAt> {
        self.iter()
            .enumerate()
            .find_map(|(project, p)| p.index_of(id).map(|workspace| WorkspaceAt { project, workspace }))
    }

    fn workspace_in(&self, project: u64, workspace: u64) -> Option<WorkspaceAt> {
        let project = self.project_index(project)?;
        let workspace = self[project].index_of(workspace)?;
        Some(WorkspaceAt { project, workspace })
    }

    fn workspace_at(&self, project: usize, workspace: usize) -> Option<WorkspaceAt> {
        (workspace < self.get(project)?.workspaces.len()).then_some(WorkspaceAt { project, workspace })
    }

    fn active_workspace(&self, project: usize) -> Option<WorkspaceAt> {
        self.workspace_at(project, self.get(project)?.active)
    }

    fn find_workspace(&self, project: usize, found: impl Fn(&Workspace) -> bool) -> Option<WorkspaceAt> {
        let workspace = self.get(project)?.workspaces.iter().position(found)?;
        Some(WorkspaceAt { project, workspace })
    }

    fn tab_index(&self, id: u64) -> Option<TabAt> {
        find_tab(self, |_, _, tab| tab.id == id)
    }

    fn tab_in(&self, project: u64, workspace: u64, tab: u64) -> Option<TabAt> {
        find_tab(self, |p, w, t| p.id == project && w.id == workspace && t.id == tab)
    }

    fn tab_at(&self, project: usize, workspace: usize, tab: usize) -> Option<TabAt> {
        let tabs = &self.get(project)?.workspaces.get(workspace)?.tabs;
        (tab < tabs.len()).then_some(TabAt { workspace: WorkspaceAt { project, workspace }, tab })
    }

    fn active_tab(&self, workspace: WorkspaceAt) -> Option<TabAt> {
        let active = self.get(workspace.project)?.workspaces.get(workspace.workspace)?.active;
        self.tab_at(workspace.project, workspace.workspace, active)
    }

    fn tab_of_pane(&self, pane: u64) -> Option<TabAt> {
        find_tab(self, |_, _, tab| tab.index_of(pane).is_some())
    }

    fn push_workspace(&mut self, project: usize, workspace: Workspace) -> WorkspaceAt {
        let workspaces = &mut self[project].workspaces;
        workspaces.push(workspace);
        WorkspaceAt { project, workspace: workspaces.len() - 1 }
    }

    fn push_tab(&mut self, workspace: WorkspaceAt, tab: Tab) -> TabAt {
        let tabs = &mut self.workspace_mut(workspace).tabs;
        tabs.push(tab);
        TabAt { workspace, tab: tabs.len() - 1 }
    }

    fn workspace(&self, at: WorkspaceAt) -> &Workspace {
        &self[at.project].workspaces[at.workspace]
    }

    fn workspace_mut(&mut self, at: WorkspaceAt) -> &mut Workspace {
        &mut self[at.project].workspaces[at.workspace]
    }

    fn workspace_by(&self, id: u64) -> Option<&Workspace> {
        Some(self.workspace(self.workspace_index(id)?))
    }

    fn workspace_by_mut(&mut self, id: u64) -> Option<&mut Workspace> {
        Some(self.workspace_mut(self.workspace_index(id)?))
    }

    fn tab(&self, at: TabAt) -> &Tab {
        &self.workspace(at.workspace).tabs[at.tab]
    }

    fn tab_mut(&mut self, at: TabAt) -> &mut Tab {
        &mut self.workspace_mut(at.workspace).tabs[at.tab]
    }

    fn project_tabs(&self, project: usize) -> impl Iterator<Item = (TabAt, &Tab)> {
        self[project].workspaces.iter().enumerate().flat_map(move |(workspace, w)| {
            let at = WorkspaceAt { project, workspace };
            w.tabs.iter().enumerate().map(move |(tab, t)| (TabAt { workspace: at, tab }, t))
        })
    }

    fn pane_by(&self, id: u64) -> Option<&Term> {
        self.tab_with_pane(id)?.1.pane_by(id)
    }

    fn pane_by_mut(&mut self, id: u64) -> Option<&mut Term> {
        self.tab_with_pane_mut(id)?.1.pane_by_mut(id)
    }

    fn tab_with_pane(&self, pane: u64) -> Option<(&Path, &Tab)> {
        let at = self.tab_of_pane(pane)?;
        Some((&self.workspace(at.workspace).path, self.tab(at)))
    }

    fn tab_with_pane_mut(&mut self, pane: u64) -> Option<(&Path, &mut Tab)> {
        let at = self.tab_of_pane(pane)?;
        let workspace = self.workspace_mut(at.workspace);
        Some((&workspace.path, &mut workspace.tabs[at.tab]))
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

fn find_tab(projects: &[Project], found: impl Fn(&Project, &Workspace, &Tab) -> bool) -> Option<TabAt> {
    projects.iter().enumerate().find_map(|(p, project)| {
        project.workspaces.iter().enumerate().find_map(|(w, workspace)| {
            let tab = workspace.tabs.iter().position(|tab| found(project, workspace, tab))?;
            Some(TabAt { workspace: WorkspaceAt { project: p, workspace: w }, tab })
        })
    })
}

pub(crate) fn group_indices(projects: &[Project], groups: &[Group]) -> Vec<Option<usize>> {
    projects.iter().map(|p| p.group.and_then(|id| groups.iter().position(|g| g.id == id))).collect()
}

pub(crate) fn sidebar_order(projects: &[Project], groups: &[Group]) -> Vec<usize> {
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
