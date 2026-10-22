use serde::{Deserialize, Serialize};

use super::Fields;
use super::arrange::PaneRef;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsClose {
    pub(crate) workspace: Option<u64>,
    pub(crate) check: bool,
    pub(crate) force: bool,
}

impl AgentsClose {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace).add("check", self.check).add("force", self.force);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsReopen {
    pub(crate) workspace: Option<u64>,
}

impl AgentsReopen {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsForget {
    pub(crate) workspace: Option<u64>,
}

impl AgentsForget {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ClosedPane {
    pub(crate) pane: u64,
    pub(crate) tab: u64,
    pub(crate) agent: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) resumes: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsClosed {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) tabs: Vec<u64>,
    pub(crate) agents: Vec<ClosedPane>,
    pub(crate) stops: Vec<PaneRef>,
    pub(crate) run: Option<u64>,
    pub(crate) closed: bool,
    pub(crate) closed_agents: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsReopened {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) tabs: Vec<u64>,
    pub(crate) panes: Vec<ClosedPane>,
    pub(crate) run: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AgentsForgot {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) forgot: usize,
    pub(crate) run: Option<u64>,
}

#[cfg(test)]
mod tests;
