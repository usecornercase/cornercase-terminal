use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::Flow;
use super::progress::Progress;

pub(crate) const MAX_MESSAGES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Send,
    Keys,
    Read,
    Wait,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) kind: Kind,
    pub(crate) text: Option<String>,
    pub(crate) bytes: usize,
    pub(crate) at: Instant,
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
        }
    }

    pub(crate) fn role_of(&self, pane: u64) -> Option<&str> {
        self.members.iter().find(|member| member.pane == pane).map(|member| member.role.as_str())
    }

    pub(crate) fn pane_of(&self, role: &str) -> Option<u64> {
        self.members.iter().find(|member| member.role == role).map(|member| member.pane)
    }

    pub(crate) fn record(&mut self, message: Message) {
        self.progress.step(&self.flow, &message);
        if self.messages.len() == MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }
}
