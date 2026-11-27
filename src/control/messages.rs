use serde::{Deserialize, Serialize};

use super::Fields;
use crate::agents::context::{Page, Told};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReadMessages {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) last: Option<usize>,
    pub(crate) before: Option<u64>,
    pub(crate) after: Option<u64>,
}

impl ReadMessages {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("pane", self.pane).maybe("tab", self.tab).maybe("last", self.last);
        fields.maybe("before", self.before).maybe("after", self.after);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Thread {
    pub(crate) pane: u64,
    pub(crate) agent: String,
    pub(crate) conversation: Option<String>,
    pub(crate) messages: Vec<Line>,
    pub(crate) more_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Line {
    pub(crate) id: u64,
    pub(crate) role: String,
    pub(crate) text: String,
    pub(crate) written: Option<String>,
}

impl Thread {
    pub(crate) fn of(pane: u64, agent: String, read: Page) -> Self {
        let messages = read.messages.into_iter().map(Line::of).collect();
        Self { pane, agent, conversation: read.conversation, messages, more_before: read.more_before }
    }
}

impl Line {
    fn of(told: Told) -> Self {
        Self { id: told.id, role: told.role.speaker().to_string(), text: told.text, written: told.written }
    }
}

#[cfg(test)]
mod tests;
