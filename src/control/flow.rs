use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::flows::Worktree;

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
    pub(crate) issue: Option<String>,
    pub(crate) source: Option<String>,
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
    #[serde(skip_serializing_if = "Worktree::is_shared")]
    pub(crate) worktree: Worktree,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MemberInfo {
    pub(crate) role: String,
    pub(crate) agent: String,
    pub(crate) tab: u64,
    pub(crate) pane: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) worktree: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branch: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowExport {
    pub(crate) name: String,
    pub(crate) project: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Exported {
    pub(crate) name: String,
    pub(crate) origin: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowImport {
    pub(crate) text: String,
    pub(crate) name: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) place: String,
    pub(crate) replace: bool,
    pub(crate) check: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Imported {
    pub(crate) name: String,
    pub(crate) origin: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) replaced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

impl FlowList {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project);
    }
}

impl FlowRun {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.name).maybe("worktree", self.worktree.as_ref()).maybe("project", self.project);
        fields.add("prompt_bytes", self.prompt.len()).add("focus", self.focus);
        fields.add("issue", self.issue.is_some()).maybe("source", self.source.as_ref());
    }
}

impl FlowExport {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.name).maybe("project", self.project);
    }
}

impl FlowImport {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("text_bytes", self.text.len()).maybe("name", self.name.as_ref());
        fields.maybe("project", self.project).add("place", &self.place);
        fields.add("replace", self.replace).add("check", self.check);
    }
}
