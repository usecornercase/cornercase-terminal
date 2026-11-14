use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::brief;
use super::progress::{Edge, Progress};
use super::{End, Flow, ORCHESTRATOR};
use crate::agents::activity::Status;
use crate::pull_request::{self, PullRequest};

pub(crate) const MAX_MESSAGES: usize = 256;
pub(crate) const MERGED_GRACE: Duration = Duration::from_secs(180);
pub(crate) const FREE_SETTLE: Duration = Duration::from_secs(5);
pub(crate) const FREE_GIVE_UP: Duration = Duration::from_secs(60);
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(60);
pub(crate) const RETRIES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Send,
    Keys,
    Read,
    Wait,
}

impl Kind {
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Keys => "keys pressed",
            Self::Read => "read",
            Self::Wait => "waited for",
            Self::Send => "sent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: Kind,
    pub(crate) text: Option<String>,
    pub(crate) bytes: usize,
    pub(crate) at: Instant,
    pub(crate) time: SystemTime,
    pub(crate) seq: u64,
    pub(crate) edge: Option<Edge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Member {
    pub(crate) role: String,
    pub(crate) pane: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RoleWorktree {
    pub(crate) role: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sent {
    pub(crate) pane: u64,
    pub(crate) at: Instant,
    pub(crate) finished: bool,
    pub(crate) failed: bool,
    pub(crate) back: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Due {
    pub(crate) role: String,
    pub(crate) pane: u64,
    pub(crate) failed: bool,
    pub(crate) back: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Retry {
    failed: Option<Instant>,
    tries: u32,
    worked: bool,
}

impl Retry {
    pub(crate) fn saw(&mut self, ended: Option<&str>, now: Instant) {
        match ended {
            Some(ended) if ended == Status::Failed.name() => {
                self.failed.get_or_insert(now);
            }
            None | Some(crate::agents::activity::SHELL) => {
                self.failed = None;
                self.worked = true;
            }
            Some(_) if self.worked => *self = Self::default(),
            Some(_) => self.failed = None,
        }
    }

    pub(crate) fn due(&self, now: Instant) -> bool {
        let wait = RETRY_AFTER * 2_u32.pow(self.tries.min(RETRIES));
        self.tries < RETRIES && self.failed.is_some_and(|at| now >= at + wait)
    }

    pub(crate) fn exhausted(&self) -> bool {
        self.tries >= RETRIES
    }

    pub(crate) fn sent(&mut self) {
        self.tries += 1;
        self.failed = None;
        self.worked = false;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Seen {
    pub(crate) gone: bool,
    pub(crate) typed: Option<Instant>,
    pub(crate) worked: bool,
    pub(crate) ended: Option<&'static str>,
    pub(crate) covered: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub(crate) id: u64,
    pub(crate) workspace: u64,
    pub(crate) flow: Flow,
    pub(crate) task: String,
    pub(crate) members: Vec<Member>,
    pub(crate) messages: VecDeque<Message>,
    pub(crate) progress: Progress,
    pub(crate) dir: Option<PathBuf>,
    pub(crate) recorded: u64,
    pub(crate) ended: Option<String>,
    pub(crate) sent: Vec<Sent>,
    pub(crate) close_when_merged: bool,
    pub(crate) free_builds: bool,
    pub(crate) merged_since: Option<Instant>,
    pub(crate) summary: bool,
    pub(crate) retry: Retry,
    pub(crate) stopped: Option<SystemTime>,
    pub(crate) exited: Vec<u64>,
}

pub(crate) fn free_due(closed: Instant, now: Instant, gone: bool) -> bool {
    let waited = now.saturating_duration_since(closed);
    waited >= FREE_SETTLE && (gone || waited >= FREE_GIVE_UP)
}

impl Run {
    pub(crate) fn new(workspace: u64, flow: Flow, task: String) -> Self {
        Self {
            id: 0,
            workspace,
            flow,
            task,
            members: Vec::new(),
            messages: VecDeque::new(),
            progress: Progress::default(),
            dir: None,
            recorded: 0,
            ended: None,
            sent: Vec::new(),
            close_when_merged: false,
            free_builds: false,
            merged_since: None,
            summary: false,
            retry: Retry::default(),
            stopped: None,
            exited: Vec::new(),
        }
    }

    pub(crate) fn role_of(&self, pane: u64) -> Option<&str> {
        self.members.iter().find(|member| member.pane == pane).map(|member| member.role.as_str())
    }

    pub(crate) fn pane_of(&self, role: &str) -> Option<u64> {
        self.members.iter().find(|member| member.role == role).map(|member| member.pane)
    }

    pub(crate) fn record(&mut self, mut message: Message) -> &Message {
        self.recorded += 1;
        message.seq = self.recorded;
        self.heard(&message);
        message.edge = self.progress.step(&self.flow, &message);
        if self.messages.len() == MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
        self.messages.back().unwrap_or_else(|| unreachable!("a message was just pushed"))
    }

    fn heard(&mut self, message: &Message) {
        if message.from != ORCHESTRATOR {
            return;
        }
        if let Some(pane) = self.pane_of(&message.to) {
            self.sent.retain(|sent| !(sent.pane == pane && sent.finished));
        }
    }

    pub(crate) fn sent_to(&mut self, pane: u64, at: Instant) {
        self.forget_sent(pane);
        if self.role_of(pane).is_some_and(|role| role != ORCHESTRATOR) {
            self.sent.push(Sent { pane, at, finished: false, failed: false, back: false });
        }
    }

    pub(crate) fn brought_back(&mut self, pane: u64, at: Instant) {
        self.forget_sent(pane);
        self.sent.push(Sent { pane, at, finished: true, failed: false, back: true });
    }

    pub(crate) fn forget_sent(&mut self, pane: u64) {
        self.sent.retain(|sent| sent.pane != pane);
    }

    pub(crate) fn settle(&mut self, pane: u64, seen: Seen) {
        let Some(index) = self.sent.iter().position(|sent| sent.pane == pane && !sent.finished) else {
            return;
        };
        let at = self.sent[index].at;
        if seen.gone || seen.typed.is_some_and(|typed| typed > at) {
            self.sent.remove(index);
            return;
        }
        let over = seen.worked && seen.ended.is_some_and(|ended| ended != Status::Waiting.name());
        if !over {
            return;
        }
        if seen.covered {
            self.sent.remove(index);
        } else {
            self.sent[index].finished = true;
            self.sent[index].failed = seen.ended == Some(Status::Failed.name());
        }
    }

    pub(crate) fn due(&self) -> Vec<u64> {
        self.sent.iter().filter(|sent| sent.finished).map(|sent| sent.pane).collect()
    }

    pub(crate) fn take_due(&mut self) -> Vec<Due> {
        let due = self
            .sent
            .iter()
            .filter(|sent| sent.finished)
            .filter_map(|sent| {
                Some(Due {
                    role: self.role_of(sent.pane)?.to_string(),
                    pane: sent.pane,
                    failed: sent.failed,
                    back: sent.back,
                })
            })
            .collect();
        self.sent.retain(|sent| !sent.finished);
        due
    }

    pub(crate) fn skipped(&self, edge: Option<Edge>) -> Option<String> {
        edge?;
        brief::owed(&self.flow, &self.progress.passed(), self.progress.changed())
    }

    pub(crate) fn owed(&self) -> Option<String> {
        brief::owed(&self.flow, self.progress.not_run(), self.progress.changed())
    }

    pub(crate) fn pull<'a>(&self, pull: Option<&'a PullRequest>) -> Option<&'a PullRequest> {
        pull.filter(|_| self.flow.end == End::PullRequest)
    }

    pub(crate) fn reached(&self, pull: Option<&PullRequest>) -> bool {
        self.ended.is_some() || self.pull(pull).is_some_and(|pull| pull.state != pull_request::State::Closed)
    }

    pub(crate) fn merged(&self, pull: Option<&PullRequest>) -> bool {
        self.pull(pull).is_some_and(|pull| pull.state == pull_request::State::Merged)
    }

    pub(crate) fn close_due(&mut self, ready: bool, now: Instant) -> bool {
        if !(ready && self.close_when_merged) {
            self.merged_since = None;
            return false;
        }
        let since = *self.merged_since.get_or_insert(now);
        now.saturating_duration_since(since) >= MERGED_GRACE
    }

    pub(crate) fn latch(&mut self, marked: bool, branch: impl FnOnce() -> Option<String>) -> bool {
        if self.ended.is_some() || !marked {
            return false;
        }
        self.ended = branch();
        self.ended.is_some()
    }

    pub(crate) fn moved_to(&mut self, branch: Option<&str>) -> Option<String> {
        let moved = matches!((&self.ended, branch), (Some(ended), Some(now)) if ended != now);
        moved.then(|| self.ended.take()).flatten()
    }
}

#[cfg(test)]
mod tests;
