use super::rows::{RemoteProject, paired_with, unpaired, workspaces_of};
use super::selection::RemoteShown;
use crate::app::App;
use crate::control::WorkspaceInfo;
use crate::project::{Project, Workspace};
use crate::ui;

pub(in crate::app) struct Column<'a> {
    pub(in crate::app) ours: &'a [Workspace],
    theirs: Vec<(&'a RemoteProject<'a>, &'a WorkspaceInfo)>,
    pub(in crate::app) alone: Option<usize>,
}

impl<'a> Column<'a> {
    pub(super) fn remote(&self, w: usize) -> Option<(&'a RemoteProject<'a>, &'a WorkspaceInfo)> {
        self.theirs.get(w.checked_sub(self.ours.len())?).copied()
    }

    fn shown(&self, shown: &RemoteShown) -> Option<(usize, Option<usize>)> {
        let mut theirs = self.theirs.iter().enumerate();
        let (k, (_, workspace)) = theirs.find(|(_, (r, w))| r.link == shown.link && w.id == shown.workspace)?;
        let tab = shown.tab.and_then(|id| workspace.tabs.iter().position(|tab| tab.id == id));
        Some((self.ours.len() + k, tab))
    }
}

fn column_of<'a>(remote: &'a [RemoteProject<'a>], local: Option<&'a Project>, alone: Option<usize>) -> Column<'a> {
    let theirs = match (alone, local) {
        (Some(k), _) => unpaired(remote).nth(k).into_iter().flat_map(workspaces_of).collect(),
        (None, Some(project)) => paired_with(remote, project.id).collect(),
        (None, None) => Vec::new(),
    };
    Column { ours: local.map(|p| p.workspaces.as_slice()).unwrap_or_default(), theirs, alone }
}

impl App {
    pub(in crate::app) fn column<'a>(&'a self, remote: &'a [RemoteProject<'a>]) -> Column<'a> {
        let shown = self.shown_remote();
        let alone = shown.and_then(|s| unpaired(remote).position(|r| r.link == s.link && r.project.id == s.project));
        column_of(remote, self.project().filter(|_| alone.is_none()), alone)
    }

    pub(in crate::app) fn column_at<'a>(&'a self, remote: &'a [RemoteProject<'a>], p: usize) -> Column<'a> {
        column_of(remote, self.projects.get(p), p.checked_sub(self.projects.len()))
    }

    pub(in crate::app) fn remote_alone(&self) -> bool {
        self.column(&self.remote_projects()).alone.is_some()
    }

    pub(in crate::app) fn remote_entries(&self, column: &Column) -> Vec<ui::WorkspaceEntry> {
        column.theirs.iter().map(|(remote, workspace)| self.remote_workspace(remote, workspace)).collect()
    }

    pub(in crate::app) fn column_entries(&self, column: &Column) -> Vec<ui::WorkspaceEntry> {
        let ours = column.ours.iter().map(|w| self.workspace_entry(w));
        ours.chain(self.remote_entries(column)).collect()
    }

    pub(in crate::app) fn column_active(&self, column: &Column) -> (usize, Option<usize>) {
        if let Some(active) = self.shown_remote().and_then(|shown| column.shown(&shown)) {
            return active;
        }
        let project = self.project().filter(|_| column.alone.is_none());
        let tabs = project.and_then(Project::workspace).filter(|w| !w.tabs.is_empty());
        (project.map_or(0, |p| p.active), tabs.map(|w| w.active))
    }
}
