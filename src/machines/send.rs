use std::time::Duration;

use super::act::{QUICK, REMOVE as WORKTREE};
use crate::control::{CloneFile, Command, FlowRun, NewWorkspace, Start, Until};
use crate::log::Job;

pub(crate) const UNWAITED: &str =
    "`wait`, `until` and `timeout` do not work with `machine`: start it there, then follow it there";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Send {
    Start(Start),
    StartWith(Start),
    NewWorkspace(NewWorkspace),
    FlowRun(FlowRun),
}

impl Send {
    pub(crate) fn command(self) -> Command {
        match self {
            Self::Start(start) => Command::Start(unwaited(start)),
            Self::StartWith(start) => Command::StartWith(unwaited(start)),
            Self::NewWorkspace(new) => Command::NewWorkspace(NewWorkspace { focus: false, machine: None, ..new }),
            Self::FlowRun(run) => Command::FlowRun(FlowRun { focus: false, machine: None, ..run }),
        }
    }

    pub(crate) fn verb(&self) -> &'static str {
        match self {
            Self::Start(_) => "start",
            Self::StartWith(_) => "start-with",
            Self::NewWorkspace(_) => "new-workspace",
            Self::FlowRun(_) => "flow-run",
        }
    }

    pub(crate) fn timeout(&self) -> Duration {
        match self {
            Self::Start(_) | Self::StartWith(_) | Self::FlowRun(_) => WORKTREE,
            Self::NewWorkspace(new) if new.worktree => WORKTREE,
            Self::NewWorkspace(_) => QUICK,
        }
    }

    pub(crate) fn logged(&self, job: Job) -> Job {
        match self.project() {
            Some(project) => job.with("project", project),
            None => job,
        }
    }

    fn project(&self) -> Option<u64> {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.project,
            Self::NewWorkspace(new) => new.project,
            Self::FlowRun(run) => run.project,
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
