use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Fields, WorktreeInfo};
use crate::control::arrange::RoleTree;

pub(crate) const REMOVED: &str = "removed";
pub(crate) const FAILED: &str = "failed";
pub(crate) const REFUSED: &str = "refused";
pub(crate) const REMOVABLE: &str = "removable";
pub(crate) const FORCE: &str = "force";
pub(crate) const UNLOCK: &str = "unlock";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own option of worktrees-remove")]
pub(crate) struct WorktreesRemove {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) check: bool,
    pub(crate) force: bool,
    pub(crate) unlock: bool,
    pub(crate) delete_branch: bool,
}

impl WorktreesRemove {
    pub(in crate::control) fn fields(&self, fields: &mut Fields) {
        let paths: Vec<String> = self.paths.iter().map(|path| path.display().to_string()).collect();
        fields.add("paths", paths.join(",")).add("check", self.check).add("force", self.force);
        fields.add("unlock", self.unlock).add("delete_branch", self.delete_branch);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorktreesRemoved {
    pub(crate) check: bool,
    pub(crate) folders: Vec<FolderRemoved>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FolderRemoved {
    pub(crate) path: PathBuf,
    pub(crate) outcome: String,
    pub(crate) why: Option<String>,
    pub(crate) needs: Vec<String>,
    pub(crate) removed_with: Option<PathBuf>,
    pub(crate) branches: Vec<String>,
    pub(crate) kept: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) lines: Vec<String>,
    pub(crate) lost: Vec<String>,
    pub(crate) unchecked: Vec<String>,
    pub(crate) teardown: bool,
    pub(crate) roles: Vec<RoleTree>,
    pub(crate) worktree: Option<WorktreeInfo>,
}

#[cfg(test)]
mod tests;
