use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NewTab {
    pub(crate) workspace: Option<u64>,
    pub(crate) name: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) focus: bool,
    pub(crate) profile: Option<String>,
    pub(crate) accounts: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Split {
    pub(crate) pane: Option<u64>,
    pub(crate) down: bool,
    pub(crate) command: Option<String>,
    pub(crate) focus: bool,
}
