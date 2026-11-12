use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::queue::outcome::Pull;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum Queue {
    Add(QueueAdd),
    Edit { id: u64, add: QueueAdd },
    List,
    Remove(u64),
    Run(u64),
    ClearStarted,
    Restore(u64),
    Pause(u64),
    Resume(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct QueueAdd {
    pub(crate) at: String,
    pub(crate) zone: String,
    pub(crate) agent: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) worktree: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) issue: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) submit: Option<bool>,
    pub(crate) accounts: BTreeMap<String, String>,
    pub(crate) repeat: Option<String>,
    pub(crate) flow: Option<String>,
    pub(crate) cleanup: Option<bool>,
    pub(crate) fresh: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct QueueShow {
    pub(crate) id: u64,
    pub(crate) conversation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QueueShown {
    #[serde(flatten)]
    pub(crate) entry: crate::queue::Listed,
    pub(crate) result: Option<ResultInfo>,
    pub(crate) messages: Option<Vec<MessageInfo>>,
    pub(crate) earlier: bool,
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ResultInfo {
    pub(crate) at: i64,
    pub(crate) text: String,
    pub(crate) cut: bool,
    pub(crate) written: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) pull_request: Option<Pull>,
    pub(crate) conversation: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MessageInfo {
    pub(crate) role: String,
    pub(crate) text: String,
    pub(crate) written: Option<String>,
    pub(crate) cut: bool,
}

#[cfg(test)]
mod tests;
