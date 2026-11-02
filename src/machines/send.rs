use std::time::Duration;

use super::act::{CHECK, QUICK, REMOVE as WORKTREE};
use crate::control::{
    AccountList, CloneFile, CloneRepository, Command, FlowList, FlowRun, IssueSources, NewTab, NewWorkspace, Start,
    Until,
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
}

impl Send {
    pub(crate) fn command(self) -> Command {
        match self {
            Self::Start(start) => Command::Start(unwaited(start)),
            Self::StartWith(start) => Command::StartWith(unwaited(start)),
            Self::NewWorkspace(new) => Command::NewWorkspace(NewWorkspace { focus: false, machine: None, ..new }),
            Self::FlowRun(run) => Command::FlowRun(FlowRun { focus: false, machine: None, ..run }),
            Self::NewTab(tab) => Command::NewTab(NewTab { focus: false, ..tab }),
            Self::Clone(clone) => Command::Clone(CloneRepository { focus: false, ..clone }),
            Self::AccountList => Command::AccountList(AccountList { check: true }),
            Self::IssueSources(project) => Command::IssueSources(IssueSources { project: Some(project) }),
            Self::FlowList(project) => Command::FlowList(FlowList { project: Some(project) }),
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
        }
    }

    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Start(_) | Self::StartWith(_) | Self::FlowRun(_) => WORKTREE,
            Self::NewWorkspace(new) if new.worktree => WORKTREE,
            Self::NewWorkspace(_) | Self::NewTab(_) | Self::IssueSources(_) | Self::FlowList(_) => QUICK,
            Self::AccountList => CHECK,
            Self::Clone(_) => CLONE,
        }
    }

    pub(crate) fn logged(&self, job: Job) -> Job {
        let job = match self.project() {
            Some(project) => job.with("project", project),
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

    fn project(&self) -> Option<u64> {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.project,
            Self::NewWorkspace(new) => new.project,
            Self::FlowRun(run) => run.project,
            Self::IssueSources(project) | Self::FlowList(project) => Some(*project),
            Self::NewTab(_) | Self::Clone(_) | Self::AccountList => None,
        }
    }
}

fn unwaited(start: Start) -> Start {
    Start { wait: false, until: Until::default(), timeout: None, focus: false, machine: None, ..start }
}

pub(crate) fn bytes(files: &[CloneFile]) -> usize {
    files.iter().map(|file| base64::decoded_len_estimate(file.data.len())).sum()
}

#[cfg(test)]
mod tests;
