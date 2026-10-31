use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, Ids};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CloneRepository {
    pub(crate) url: String,
    pub(crate) parent: PathBuf,
    pub(crate) focus: bool,
}

impl CloneRepository {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("parent", self.parent.display()).add("focus", self.focus);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Cloned {
    pub(crate) path: PathBuf,
    #[serde(flatten)]
    pub(crate) ids: Ids,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection: Option<u64>,
}
