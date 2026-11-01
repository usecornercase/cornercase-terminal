use std::path::PathBuf;
use std::time::SystemTime;

use crate::agents::activity::{self, Status};
use crate::app::App;
use crate::control::{PaneInfo, ProjectInfo, TabInfo, WorkspaceInfo};
use crate::machines::{self, Info, State};
use crate::ui;

const STATUSES: [Status; 6] =
    [Status::Idle, Status::Working, Status::Done, Status::Pending, Status::Failed, Status::Waiting];

pub(in crate::app) struct RemoteProject<'a> {
    pub(super) link: u64,
    pub(super) tag: ui::MachineTag,
    pub(super) project: &'a ProjectInfo,
    pub(super) paired: Option<u64>,
}

impl App {
    pub(in crate::app) fn remote_projects(&self) -> Vec<RemoteProject<'_>> {
        let now = SystemTime::now();
        let held = self.machines_held().into_iter();
        let seen = held.filter_map(|(info, held)| Some((info, held.link.id(), held.seen.as_deref()?)));
        seen.flat_map(|(info, link, seen)| {
            let tag = tag(&info, now);
            let paired = self.paired(seen);
            seen.projects.iter().zip(paired).map(move |(project, paired)| RemoteProject {
                link,
                tag: tag.clone(),
                project,
                paired,
            })
        })
        .collect()
    }

    pub(super) fn remote_workspace(&self, remote: &RemoteProject, workspace: &WorkspaceInfo) -> ui::WorkspaceEntry {
        let folded = |(machine, path): &(String, PathBuf)| *machine == remote.tag.name && *path == workspace.path;
        ui::WorkspaceEntry {
            name: workspace.name.clone(),
            collapsed: self.machines.folded.iter().any(folded),
            tabs: workspace.tabs.iter().map(|tab| self.remote_tab(tab)).collect(),
            machine: Some(remote.tag.clone()),
            ..ui::WorkspaceEntry::default()
        }
    }

    pub(in crate::app) fn paired_entries(&self, remote: &[RemoteProject], project: u64) -> Vec<ui::WorkspaceEntry> {
        paired_with(remote, project).map(|(r, workspace)| self.remote_workspace(r, workspace)).collect()
    }

    pub(in crate::app) fn alone_entries<'a>(
        &'a self,
        remote: &'a [RemoteProject],
    ) -> impl Iterator<Item = Vec<ui::WorkspaceEntry>> + 'a {
        unpaired(remote).map(|r| workspaces_of(r).map(|(r, workspace)| self.remote_workspace(r, workspace)).collect())
    }

    fn remote_tab(&self, tab: &TabInfo) -> ui::TabEntry {
        let known = |pane: &&PaneInfo| pane.model.is_some() || pane.compacted;
        let shown = tab.panes.iter().find(|pane| pane.active).filter(known).or_else(|| tab.panes.iter().find(known));
        let memory = tab.panes.iter().filter_map(|pane| pane.memory).reduce(u64::saturating_add);
        let subagents = tab.panes.iter().map(|pane| usize::try_from(pane.subagents).unwrap_or(usize::MAX)).sum();
        let context = (shown.and_then(|pane| pane.model.clone()), shown.and_then(|pane| pane.context));
        let details = self.context_details(context, memory, subagents, None);
        ui::TabEntry {
            name: tab.name.clone(),
            status: status(tab.status.as_deref()),
            running: tab.panes.iter().any(|pane| pane.at_prompt == Some(false)),
            details: ui::Details {
                compacted: self.shows_compaction(shown.is_some_and(|pane| pane.compacted)),
                ..details
            },
            others: tab.panes.len().saturating_sub(1),
        }
    }
}

pub(in crate::app) fn paired<'a>(remote: &'a [RemoteProject], project: u64) -> impl Iterator<Item = &'a WorkspaceInfo> {
    paired_with(remote, project).map(|(_, workspace)| workspace)
}

pub(super) fn paired_with<'a, 'b>(
    remote: &'a [RemoteProject<'b>],
    project: u64,
) -> impl Iterator<Item = (&'a RemoteProject<'b>, &'b WorkspaceInfo)> {
    remote.iter().filter(move |r| r.paired == Some(project)).flat_map(workspaces_of)
}

pub(super) fn workspaces_of<'a, 'b>(
    remote: &'a RemoteProject<'b>,
) -> impl Iterator<Item = (&'a RemoteProject<'b>, &'b WorkspaceInfo)> {
    remote.project.workspaces.iter().map(move |workspace| (remote, workspace))
}

pub(in crate::app) fn statuses<'a>(
    workspaces: impl Iterator<Item = &'a WorkspaceInfo>,
) -> impl Iterator<Item = Option<Status>> {
    workspaces.flat_map(|w| &w.tabs).map(|tab| status(tab.status.as_deref()))
}

pub(super) fn unpaired<'a, 'b>(remote: &'a [RemoteProject<'b>]) -> impl Iterator<Item = &'a RemoteProject<'b>> {
    remote.iter().filter(|r| r.paired.is_none())
}

pub(in crate::app) fn remote_only<'a>(remote: &'a [RemoteProject]) -> impl Iterator<Item = ui::ProjectEntry> + 'a {
    unpaired(remote).map(|r| ui::ProjectEntry {
        name: r.project.name.clone(),
        workspaces: r.project.workspaces.len(),
        group: None,
        status: activity::attention(statuses(r.project.workspaces.iter())),
        machine: Some(r.tag.clone()),
    })
}

pub(in crate::app) fn machine_shape(entry: &ui::WorkspaceEntry, open: bool) -> ui::WorkspaceShape {
    let tabs = if open && !entry.collapsed { entry.tab_lines() } else { Vec::new() };
    ui::WorkspaceShape { collapsed: entry.collapsed, machine: true, tabs }
}

pub(in crate::app) fn machine_project(entries: &[ui::WorkspaceEntry]) -> ui::ProjectShape {
    let workspaces = entries.iter().map(|entry| machine_shape(entry, true)).collect();
    ui::ProjectShape { group: None, collapsed: false, machine: true, workspaces }
}

pub(super) fn status(name: Option<&str>) -> Option<Status> {
    STATUSES.into_iter().find(|status| Some(status.name()) == name)
}

fn tag(info: &Info, now: SystemTime) -> ui::MachineTag {
    ui::MachineTag {
        icon: info.icon.clone(),
        name: info.name.clone(),
        dim: info.state != State::Linked,
        seen: info.seen.as_deref().and_then(|seen| machines::seen_ago(seen, now)),
    }
}
