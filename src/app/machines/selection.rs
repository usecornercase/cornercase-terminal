use std::path::PathBuf;

use crate::app::App;
use crate::control::{self, ProjectInfo, WorkspaceInfo};
use crate::log;
use crate::project::Hierarchy;

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
        named.map(|held| held.link.id()).is_some_and(|link| self.select_on(link, item))
    }

    pub(in crate::app) fn select_on(&mut self, link: u64, item: RemoteItem) -> bool {
        let linked = self.machines.held.iter().find(|held| held.link.id() == link);
        let Some((held, seen)) = linked.and_then(|held| Some((held, held.seen.as_deref()?))) else { return false };
        let found = match item {
            RemoteItem::Workspace(id) => place(seen, |w| w.id == id).map(|(p, w)| (p, w, active_tab(w))),
            RemoteItem::Tab(id) => place(seen, |w| w.tabs.iter().any(|t| t.id == id)).map(|(p, w)| (p, w, Some(id))),
        };
        let Some((project, workspace, tab)) = found else { return false };
        log::info!("machines", "remote selected", name = held.name, workspace = workspace.id);
        self.machines.selection = Some(selected(link, seen, project, workspace, tab));
        self.panels.files.set_full_screen(false);
        self.follow_pairing();
        true
    }

    pub(super) fn follow_pairing(&mut self) {
        let Some(shown) = self.shown_remote() else { return };
        if let Some(p) = self.paired_index(shown.link, shown.project).filter(|&p| p != self.active) {
            log::info!("machines", "paired project followed", project = self.projects[p].id);
            self.active = p;
        }
    }

    fn paired_index(&self, link: u64, project: u64) -> Option<usize> {
        let remote = self.remote_projects();
        let paired = remote.iter().find(|r| r.link == link && r.project.id == project)?.paired?;
        self.projects.project_index(paired)
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
        self.follow_pairing();
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
