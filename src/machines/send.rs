use std::time::Duration;

use serde_json::Value;

use super::act::{self, CHECK, QUICK, REMOVE as WORKTREE};
use super::run_on;
use crate::control::contract::{Code, Refusal};
use crate::control::{
    AccountList, CloneFile, CloneRepository, Command, FlowList, FlowRun, Fold, IssueSources, Item, NewTab,
    NewWorkspace, Start, Until,
};
use crate::log::Job;

const CLONE: Duration = Duration::from_mins(30);

pub(crate) const UNWAITED: &str =
    "`wait`, `until` and `timeout` do not work with `machine`: start it there, then follow it there";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Send {
    Start(Start),
    StartWith(Start),
    NewWorkspace(NewWorkspace),
    FlowRun(FlowRun),
    NewTab(NewTab),
    Clone(CloneRepository),
    AccountList,
    IssueSources(u64),
    FlowList(u64),
    Fold { pane: u64, collapse: bool },
}

pub(crate) fn folding(collapse: bool) -> &'static str {
    if collapse { "collapse" } else { "expand" }
}

impl Send {
    pub(crate) fn command(self) -> Command {
        match self {
            Self::Start(start) => Command::Start(unwaited(start)),
            Self::StartWith(start) => Command::StartWith(unwaited(start)),
            Self::NewWorkspace(new) => {
                Command::NewWorkspace(NewWorkspace { focus: false, machine: None, machine_boot: None, ..new })
            }
            Self::FlowRun(run) => Command::FlowRun(FlowRun { focus: false, machine: None, machine_boot: None, ..run }),
            Self::NewTab(tab) => Command::NewTab(NewTab { focus: false, ..tab }),
            Self::Clone(clone) => Command::Clone(CloneRepository { focus: false, ..clone }),
            Self::AccountList => Command::AccountList(AccountList { check: true }),
            Self::IssueSources(project) => Command::IssueSources(IssueSources { project: Some(project) }),
            Self::FlowList(project) => Command::FlowList(FlowList { project: Some(project) }),
            Self::Fold { pane, collapse: true } => Command::Collapse(Fold { item: Item::Pane(pane) }),
            Self::Fold { pane, collapse: false } => Command::Expand(Fold { item: Item::Pane(pane) }),
        }
    }

    pub(crate) fn hearing(&self) -> fn(&str) -> Result<Value, Refusal> {
        match self {
            Self::FlowList(_) => run_on::heard_flows,
            _ => act::heard,
        }
    }

    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Self::Start(_) => "start",
            Self::StartWith(_) => "start-with",
            Self::NewWorkspace(_) => "new-workspace",
            Self::FlowRun(_) => "flow-run",
            Self::NewTab(_) => "new-tab",
            Self::Clone(_) => "clone",
            Self::AccountList => "account-list",
            Self::IssueSources(_) => "issue-sources",
            Self::FlowList(_) => "flow-list",
            Self::Fold { collapse, .. } => folding(*collapse),
        }
    }

    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Start(_) | Self::StartWith(_) | Self::FlowRun(_) => WORKTREE,
            Self::NewWorkspace(new) if new.worktree => WORKTREE,
            Self::NewWorkspace(_) | Self::NewTab(_) | Self::IssueSources(_) | Self::FlowList(_) | Self::Fold { .. } => {
                QUICK
            }
            Self::AccountList => CHECK,
            Self::Clone(_) => CLONE,
        }
    }

    pub(crate) fn focus(&self) -> bool {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.focus,
            Self::NewWorkspace(new) => new.focus,
            Self::FlowRun(run) => run.focus,
            Self::NewTab(tab) => tab.focus,
            Self::Clone(clone) => clone.focus,
            Self::AccountList | Self::IssueSources(_) | Self::FlowList(_) | Self::Fold { .. } => false,
        }
    }

    pub(crate) fn machine_boot(&self) -> Option<&str> {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.machine_boot.as_deref(),
            Self::NewWorkspace(new) => new.machine_boot.as_deref(),
            Self::FlowRun(run) => run.machine_boot.as_deref(),
            Self::NewTab(_)
            | Self::Clone(_)
            | Self::AccountList
            | Self::IssueSources(_)
            | Self::FlowList(_)
            | Self::Fold { .. } => None,
        }
    }

    pub(crate) fn logged(&self, job: Job) -> Job {
        let job = match self.about() {
            Some((key, id)) => job.with(key, id),
            None => job,
        };
        match self {
            Self::NewTab(NewTab { workspace: Some(workspace), .. }) => job.with("workspace", workspace),
            Self::Clone(clone) if !clone.files.is_empty() => {
                job.with("files", clone.files.len()).with("bytes", bytes(&clone.files))
            }
            _ => job,
        }
    }

    fn about(&self) -> Option<(&'static str, u64)> {
        match self {
            Self::Fold { pane, .. } => Some(("pane", *pane)),
            _ => self.project().map(|project| ("project", project)),
        }
    }

    fn project(&self) -> Option<u64> {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.project,
            Self::NewWorkspace(new) => new.project,
            Self::FlowRun(run) => run.project,
            Self::IssueSources(project) | Self::FlowList(project) => Some(*project),
            Self::NewTab(_) | Self::Clone(_) | Self::AccountList | Self::Fold { .. } => None,
        }
    }
}

pub(crate) fn same_run(machine: &str, asked: Option<&str>, reported: Option<&str>) -> Result<(), Refusal> {
    match (asked, reported) {
        (Some(asked), Some(reported)) if asked != reported => Err(Refusal::new(
            Code::StaleBoot,
            format!(
                "this request is for the cornercase server started as {asked} on {machine}, but {machine} last \
                reported from the one started as {reported}; its ids may name other items now, so read \
                `cornercase status` again"
            ),
        )),
        _ => Ok(()),
    }
}

fn unwaited(start: Start) -> Start {
    Start {
        wait: false,
        until: Until::default(),
        timeout: None,
        focus: false,
        machine: None,
        machine_boot: None,
        ..start
    }
}

pub(crate) fn bytes(files: &[CloneFile]) -> u64 {
    files.iter().map(CloneFile::size).sum()
}

#[cfg(test)]
mod tests;
