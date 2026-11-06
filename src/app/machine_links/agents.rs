use ratatui::layout::Position;
use serde_json::Value;

use super::RemoteAt;
use super::acting::{refused, sighted_pane};
use super::relay::{Sent, Then};
use super::rows::{RemoteProject, paired_with, status, unpaired, workspaces_of};
use super::selection::RemoteShown;
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::agent_watch::AgentKey;
use crate::app::dialogs::named;
use crate::app::menus::MenuAction;
use crate::control::contract::Refusal;
use crate::control::{PaneInfo, ProjectInfo, WorkspaceInfo};
use crate::flows;
use crate::machines::send::{Send, folding};
use crate::ui;

const AGENT: &str = "the agent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct PaneAim {
    pub(in crate::app) link: u64,
    pub(in crate::app) era: u64,
    pub(in crate::app) pane: u64,
    pub(in crate::app) collapse: bool,
}

pub(in crate::app) struct ThereAgent<'a> {
    pub(in crate::app) link: u64,
    era: u64,
    pub(in crate::app) tag: ui::MachineTag,
    pub(in crate::app) paired: Option<u64>,
    pub(in crate::app) project: &'a ProjectInfo,
    pub(in crate::app) workspace: &'a WorkspaceInfo,
    tab: u64,
    pub(in crate::app) pane: &'a PaneInfo,
    pub(in crate::app) status: Status,
}

impl ThereAgent<'_> {
    pub(in crate::app) fn row(&self) -> (AgentKey, Option<AgentKey>) {
        ((Some(self.link), self.pane.id), self.pane.parent.map(|parent| (Some(self.link), parent)))
    }

    pub(in crate::app) fn seat(&self, depth: usize) -> (Option<String>, bool) {
        let Some(role) = self.pane.role.clone() else { return (None, true) };
        let panes = || self.workspace.tabs.iter().flat_map(|tab| &tab.panes);
        let parent = self.pane.parent.and_then(|parent| panes().find(|pane| pane.id == parent));
        let under = depth > 0 && role != flows::ORCHESTRATOR && parent.is_some_and(|parent| parent.role.is_some());
        (Some(role), !under)
    }

    pub(in crate::app) fn at(&self) -> RemoteAt {
        let aim = PaneAim { link: self.link, era: self.era, pane: self.pane.id, collapse: !self.pane.collapsed };
        RemoteAt::Agent { workspace: self.workspace.id, tab: self.tab, aim }
    }

    pub(in crate::app) fn shown_in(&self, shown: &RemoteShown) -> bool {
        shown.link == self.link && shown.tab == Some(self.tab) && self.pane.active
    }
}

impl App {
    pub(in crate::app) fn machine_agents<'a>(&self, remote: &[RemoteProject<'a>], project: u64) -> Vec<ThereAgent<'a>> {
        self.there_agents(paired_with(remote, project))
    }

    pub(in crate::app) fn alone_agents<'a>(&self, remote: &[RemoteProject<'a>]) -> Vec<ThereAgent<'a>> {
        self.there_agents(unpaired(remote).flat_map(workspaces_of))
    }

    fn there_agents<'a, 'r>(
        &self,
        workspaces: impl Iterator<Item = (&'r RemoteProject<'a>, &'a WorkspaceInfo)>,
    ) -> Vec<ThereAgent<'a>>
    where
        'a: 'r,
    {
        let kept = workspaces.filter(|(remote, workspace)| !self.machines.acting.removing(remote.link, workspace.id));
        kept.flat_map(|(remote, workspace)| {
            let panes = workspace.tabs.iter().flat_map(|tab| tab.panes.iter().map(move |pane| (tab.id, pane)));
            panes.filter_map(move |(tab, pane)| {
                Some(ThereAgent {
                    link: remote.link,
                    era: remote.era,
                    tag: remote.tag.clone(),
                    paired: remote.paired,
                    project: remote.project,
                    workspace,
                    tab,
                    pane,
                    status: status(pane.status.as_deref())?,
                })
            })
        })
        .collect()
    }

    pub(in crate::app) fn send_agent_fold(&mut self, aim: PaneAim) {
        let PaneAim { pane, collapse, .. } = aim;
        let sent = self.aimed_pane(aim).and_then(|machine| {
            let send = Send::Fold { pane, collapse };
            self.send_to_machine(&machine, send, Then::Fold { collapse }).map_err(|refusal| refusal.message)
        });
        if let Err(why) = sent {
            self.not_sent(&why);
        }
    }

    pub(in crate::app) fn agent_fold_menu(&mut self, aim: PaneAim, at: Position) {
        self.noted_menu(aim.link, MenuAction::RemoteAgentFold(aim), at);
    }

    fn aimed_pane(&self, aim: PaneAim) -> Result<String, String> {
        let held = self.linked_held(aim.link)?;
        sighted_pane(held, aim.era, aim.pane)?;
        Ok(held.name.clone())
    }

    pub(super) fn fold_relayed(&mut self, sent: &Sent, collapse: bool, result: Result<Value, Refusal>) {
        if let Err(refusal) = result {
            let named = named(AGENT, Some(&sent.machine));
            self.messages.toast = Some(refused(folding(collapse), &named, &sent.machine, &refusal));
        }
    }
}
