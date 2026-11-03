use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, Ids};
use crate::git::clone::files::PLAIN_MODE;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CloneRepository {
    pub(crate) url: String,
    pub(crate) parent: PathBuf,
    pub(crate) focus: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) files: Vec<CloneFile>,
}

impl CloneRepository {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.add("parent", self.parent.display()).add("focus", self.focus);
        fields.maybe("files", (!self.files.is_empty()).then_some(self.files.len()));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CloneFile {
    #[serde(default)]
    pub(crate) path: PathBuf,
    #[serde(default)]
    pub(crate) data: String,
    #[serde(default = "plain_mode")]
    pub(crate) mode: u32,
}

impl CloneFile {
    pub(crate) fn size(&self) -> u64 {
        let data = self.data.as_bytes();
        let padding = data.iter().rev().take(2).take_while(|&&byte| byte == b'=').count();
        (data.len() / 4 * 3).saturating_sub(padding) as u64
    }
}

const fn plain_mode() -> u32 {
    PLAIN_MODE
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Cloned {
    pub(crate) path: PathBuf,
    #[serde(flatten)]
    pub(crate) ids: Ids,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) unwritten: Vec<Unwritten>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Unwritten {
    pub(crate) path: PathBuf,
    pub(crate) why: String,
}
