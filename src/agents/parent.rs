use serde::{Deserialize, Serialize};

use super::AgentState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Parent {
    pub(crate) pane: u64,
    pub(crate) by: By,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum By {
    Conversation(Conversation),
    Agent(i32),
    Shell(i32),
    Pane,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Conversation {
    pub(crate) kind: String,
    pub(crate) conversation: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Running<'a> {
    pub(crate) conversation: Option<&'a AgentState>,
    pub(crate) read: Option<i32>,
    pub(crate) agent: Option<i32>,
    pub(crate) shell: Option<i32>,
}

impl Conversation {
    pub(crate) fn of(state: &AgentState) -> Self {
        Self { kind: state.kind.clone(), conversation: state.conversation.clone() }
    }

    pub(crate) fn is(&self, state: &AgentState) -> bool {
        self.kind == state.kind && self.conversation == state.conversation
    }
}

impl Parent {
    pub(crate) fn pane(pane: u64) -> Self {
        Self { pane, by: By::Pane }
    }
}

impl Running<'_> {
    pub(crate) fn by(self) -> Option<By> {
        let Some(pid) = self.agent else { return self.shell.map(By::Shell) };
        let known = self.conversation.filter(|_| self.read == Some(pid));
        Some(known.map_or(By::Agent(pid), |state| By::Conversation(Conversation::of(state))))
    }
}

impl By {
    pub(crate) fn holds(&self, now: Running<'_>) -> bool {
        match self {
            Self::Conversation(conversation) => now.conversation.is_some_and(|state| conversation.is(state)),
            Self::Agent(pid) => now.agent == Some(*pid),
            Self::Shell(pid) => now.shell == Some(*pid) && now.agent.is_none() && now.conversation.is_none(),
            Self::Pane => true,
        }
    }

    pub(crate) fn learn(&mut self, (pid, conversation): (i32, &Conversation)) {
        if *self == Self::Agent(pid) {
            *self = Self::Conversation(conversation.clone());
        }
    }

    pub(crate) fn kept(&self, now: Running<'_>) -> bool {
        self.conversation().is_some() && self.holds(now)
    }

    pub(crate) fn lasts(&self) -> bool {
        matches!(self, Self::Conversation(_) | Self::Pane)
    }

    pub(crate) fn conversation(&self) -> Option<&Conversation> {
        match self {
            Self::Conversation(conversation) => Some(conversation),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
