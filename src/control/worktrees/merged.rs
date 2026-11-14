use serde::{Deserialize, Serialize};

use super::{BuildsFreed, Fields};

pub(crate) use super::remove::{FAILED, REFUSED};

pub(crate) const FREED: &str = "freed";
pub(crate) const FREEABLE: &str = "freeable";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FreeMergedBuilds {
    pub(crate) project: Option<u64>,
    pub(crate) check: bool,
}

impl FreeMergedBuilds {
    pub(in crate::control) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project);
        fields.add("check", self.check);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorktreeFreed {
    pub(crate) outcome: String,
    pub(crate) why: Option<String>,
    #[serde(flatten)]
    pub(crate) freed: BuildsFreed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MergedBuildsFreed {
    pub(crate) check: bool,
    pub(crate) bytes: u64,
    pub(crate) worktrees: Vec<WorktreeFreed>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MergedTotal {
    pub(crate) bytes: u64,
    pub(crate) count: usize,
}

#[cfg(test)]
mod tests;
