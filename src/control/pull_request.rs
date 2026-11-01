use serde::{Deserialize, Serialize};

use super::{Fields, PullRequestInfo};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PullRequestRead {
    pub(crate) workspace: Option<u64>,
}

impl PullRequestRead {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PullRequestAnswer {
    pub(crate) workspace: u64,
    pub(crate) branch: Option<String>,
    pub(crate) pull_request: Option<PullRequestInfo>,
    pub(crate) reason: Option<String>,
}

#[cfg(test)]
mod tests;
