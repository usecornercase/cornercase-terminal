use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowList {
    pub(crate) project: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowRun {
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) worktree: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Flows {
    pub(crate) flows: Vec<FlowInfo>,
    pub(crate) broken: Vec<BrokenFlow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowInfo {
    pub(crate) name: String,
    pub(crate) origin: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) description: String,
    pub(crate) roles: Vec<RoleInfo>,
    pub(crate) problems: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) check: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RoleInfo {
    pub(crate) role: String,
    pub(crate) agent: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) edits: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BrokenFlow {
    pub(crate) path: PathBuf,
    pub(crate) error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RunInfo {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) branch: String,
    pub(crate) members: Vec<MemberInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MemberInfo {
    pub(crate) role: String,
    pub(crate) agent: String,
    pub(crate) tab: u64,
    pub(crate) pane: u64,
}
