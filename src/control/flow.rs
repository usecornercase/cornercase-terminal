use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::flows::{self, Back, End, Flow, Role, Stage, Worktree};

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
#[expect(clippy::struct_field_names, reason = "the answer's JSON key is `flows`")]
pub(crate) struct Flows {
    pub(crate) flows: Vec<FlowInfo>,
    pub(crate) broken: Vec<BrokenFlow>,
    pub(crate) hidden: Vec<FlowInfo>,
    pub(crate) builtins: Vec<String>,
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
    pub(crate) flow: FlowSpec,
    pub(crate) edited: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowSpec {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) notes: String,
    pub(crate) end: End,
    pub(crate) orchestrator: RoleSpec,
    pub(crate) roles: Vec<RoleSpec>,
    pub(crate) stages: Vec<StageSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RoleSpec {
    pub(crate) name: String,
    pub(crate) agent: String,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) edits: bool,
    pub(crate) worktree: Worktree,
    pub(crate) prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct StageSpec {
    pub(crate) roles: Vec<String>,
    pub(crate) back: Option<BackSpec>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BackSpec {
    pub(crate) to: usize,
    pub(crate) when: String,
    pub(crate) at_most: u32,
}

impl From<&Role> for RoleSpec {
    fn from(role: &Role) -> Self {
        Self {
            name: role.name.clone(),
            agent: role.agent.clone(),
            model: role.model.clone(),
            effort: role.effort.clone(),
            args: role.args.clone(),
            env: role.env.clone(),
            edits: role.edits,
            worktree: role.worktree,
            prompt: role.prompt.clone(),
        }
    }
}

impl From<&Flow> for FlowSpec {
    fn from(flow: &Flow) -> Self {
        let stage = |stage: &Stage| StageSpec {
            roles: stage.roles.clone(),
            back: stage.back.as_ref().map(|back| BackSpec {
                to: back.to,
                when: back.when.clone(),
                at_most: back.at_most,
            }),
        };
        Self {
            name: flow.name.clone(),
            description: flow.description.clone(),
            notes: flow.notes.clone(),
            end: flow.end,
            orchestrator: (&flow.orchestrator).into(),
            roles: flow.roles.iter().map(Into::into).collect(),
            stages: flow.stages.iter().map(stage).collect(),
        }
    }
}

impl RoleSpec {
    fn into_role(self) -> Role {
        Role {
            name: self.name,
            agent: self.agent,
            model: self.model,
            effort: self.effort,
            args: self.args,
            env: self.env,
            edits: self.edits,
            worktree: self.worktree,
            prompt: self.prompt,
        }
    }
}

impl FlowSpec {
    pub(crate) fn into_flow(self) -> Flow {
        let mut orchestrator = self.orchestrator.into_role();
        orchestrator.name = flows::ORCHESTRATOR.into();
        let stage = |stage: StageSpec| Stage {
            roles: stage.roles,
            back: stage.back.map(|back| Back { to: back.to, when: back.when, at_most: back.at_most }),
        };
        Flow {
            version: flows::VERSION,
            name: self.name,
            description: self.description,
            notes: self.notes,
            end: self.end,
            orchestrator,
            roles: self.roles.into_iter().map(RoleSpec::into_role).collect(),
            stages: self.stages.into_iter().map(stage).collect(),
        }
    }
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
    pub(crate) run: u64,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowSave {
    pub(crate) flow: FlowSpec,
    pub(crate) place: String,
    pub(crate) project: Option<u64>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) check: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowDelete {
    pub(crate) name: String,
    pub(crate) place: String,
    pub(crate) project: Option<u64>,
    pub(crate) path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowHide {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowRestore {
    pub(crate) name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowSaved {
    pub(crate) name: String,
    pub(crate) origin: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) saved: bool,
    pub(crate) problems: Vec<String>,
    pub(crate) check: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FlowRemoved {
    pub(crate) name: String,
    pub(crate) origin: Option<String>,
    pub(crate) deleted: Option<PathBuf>,
    pub(crate) hidden: bool,
    pub(crate) changed: bool,
}

impl FlowSave {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.flow.name).add("place", &self.place).maybe("project", self.project);
        fields.add("previous", self.path.is_some()).add("check", self.check);
    }
}

impl FlowDelete {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.name).add("place", &self.place).maybe("project", self.project);
        fields.add("path", self.path.is_some());
    }
}

impl FlowHide {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.name);
    }
}

impl FlowRestore {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("flow", &self.name);
    }
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

#[cfg(test)]
mod tests;
