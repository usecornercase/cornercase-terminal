use std::path::PathBuf;

use crate::app::App;
use crate::control::{self, ProjectInfo, WorkspaceInfo};
use crate::log;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct RemoteSelection {
    link: u64,
    boot: String,
    project: u64,
    workspace: u64,
    tab: Option<u64>,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct RemoteShown {
    pub(in crate::app) machine: String,
    pub(in crate::app) link: u64,
    pub(in crate::app) boot: String,
    pub(in crate::app) project: u64,
    pub(in crate::app) workspace: u64,
    pub(in crate::app) tab: Option<u64>,
    pub(in crate::app) path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum RemoteItem {
    Workspace(u64),
    Tab(u64),
}

impl App {
    pub(in crate::app) fn shown_remote(&self) -> Option<RemoteShown> {
        let selection = self.machines.selection.as_ref()?;
        let held = self.machines.held.iter().find(|held| held.link.id() == selection.link)?;
        held.seen.as_deref().filter(|seen| seen.boot == selection.boot)?;
        Some(RemoteShown {
            machine: held.name.clone(),
            link: selection.link,
            boot: selection.boot.clone(),
            project: selection.project,
            workspace: selection.workspace,
            tab: selection.tab,
            path: selection.path.clone(),
        })
    }

    pub(in crate::app) fn select_remote(&mut self, machine: &str, item: RemoteItem) -> bool {
        let named = self.machines.held.iter().find(|held| held.name.eq_ignore_ascii_case(machine));
        let Some((held, seen)) = named.and_then(|held| Some((held, held.seen.as_deref()?))) else { return false };
        let found = match item {
            RemoteItem::Workspace(id) => place(seen, |w| w.id == id).map(|(p, w)| (p, w, active_tab(w))),
            RemoteItem::Tab(id) => place(seen, |w| w.tabs.iter().any(|t| t.id == id)).map(|(p, w)| (p, w, Some(id))),
        };
        let Some((project, workspace, tab)) = found else { return false };
        log::info!("machines", "remote selected", name = held.name, workspace = workspace.id);
        let link = held.link.id();
        self.machines.selection = Some(selected(link, seen, project, workspace, tab));
        true
    }

    pub(in crate::app) fn sent_workspaces(&self, machine: &str) -> bool {
        self.machines.held.iter().any(|held| held.name.eq_ignore_ascii_case(machine) && held.seen.is_some())
    }

    pub(in crate::app) fn leave_remote(&mut self) {
        self.machines.selection = None;
    }

    pub(super) fn follow_selection(&mut self) {
        let Some(selection) = &self.machines.selection else { return };
        let held = self.machines.held.iter().find(|held| held.link.id() == selection.link);
        let kept = held.and_then(|held| followed(selection, held.link.id(), held.seen.as_deref()?));
        if kept.is_none() {
            log::info!("machines", "remote selection dropped", link = selection.link);
        }
        self.machines.selection = kept;
    }
}

fn followed(selection: &RemoteSelection, link: u64, seen: &control::Report) -> Option<RemoteSelection> {
    if seen.boot == selection.boot {
        let (project, workspace) = place(seen, |w| w.id == selection.workspace)?;
        let tab =
            selection.tab.filter(|id| workspace.tabs.iter().any(|t| t.id == *id)).or_else(|| active_tab(workspace));
        return Some(selected(link, seen, project, workspace, tab));
    }
    let (project, workspace) = place(seen, |w| w.path == selection.path)?;
    Some(selected(link, seen, project, workspace, active_tab(workspace)))
}

fn selected(
    link: u64,
    seen: &control::Report,
    project: &ProjectInfo,
    workspace: &WorkspaceInfo,
    tab: Option<u64>,
) -> RemoteSelection {
    RemoteSelection {
        link,
        boot: seen.boot.clone(),
        project: project.id,
        workspace: workspace.id,
        tab,
        path: workspace.path.clone(),
    }
}

fn place(seen: &control::Report, wanted: impl Fn(&WorkspaceInfo) -> bool) -> Option<(&ProjectInfo, &WorkspaceInfo)> {
    seen.projects.iter().find_map(|project| project.workspaces.iter().find(|w| wanted(w)).map(|w| (project, w)))
}

fn active_tab(workspace: &WorkspaceInfo) -> Option<u64> {
    workspace.tabs.iter().find(|tab| tab.active).or_else(|| workspace.tabs.first()).map(|tab| tab.id)
}
