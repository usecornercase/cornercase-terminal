use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Fields;
use crate::worktrees::builds::{Folder, Freed, Size};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorktreeSize {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: Option<PathBuf>,
}

impl WorktreeSize {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
        fields.maybe("path", self.path.as_ref().map(|path| path.display()));
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FreeBuilds {
    pub(crate) workspace: Option<u64>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) check: bool,
    pub(crate) force: bool,
}

impl FreeBuilds {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("workspace", self.workspace);
        fields.maybe("path", self.path.as_ref().map(|path| path.display()));
        fields.add("check", self.check);
        fields.add("force", self.force);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BuildFolder {
    pub(crate) path: String,
    pub(crate) bytes: u64,
}

impl BuildFolder {
    fn of(folder: &Folder) -> Self {
        Self { path: folder.path.display().to_string(), bytes: folder.bytes }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FailedFolder {
    pub(crate) path: String,
    pub(crate) error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorktreeSized {
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
    pub(crate) builds_bytes: u64,
    pub(crate) builds: Vec<BuildFolder>,
    pub(crate) nested: Vec<String>,
}

impl WorktreeSized {
    pub(crate) fn of(path: PathBuf, size: &Size) -> Self {
        Self {
            path,
            bytes: size.bytes,
            builds_bytes: size.builds_bytes(),
            builds: size.builds.iter().map(BuildFolder::of).collect(),
            nested: size.nested.iter().map(|path| path.display().to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct BuildsFreed {
    pub(crate) path: PathBuf,
    pub(crate) check: bool,
    pub(crate) bytes: u64,
    pub(crate) folders: Vec<BuildFolder>,
    pub(crate) kept: Vec<String>,
    pub(crate) failed: Vec<FailedFolder>,
    pub(crate) running: Vec<String>,
}

impl BuildsFreed {
    pub(crate) fn would(path: PathBuf, size: &Size, running: Vec<String>) -> Self {
        let folders: Vec<BuildFolder> = size.builds.iter().map(BuildFolder::of).collect();
        Self { path, check: true, bytes: size.builds_bytes(), folders, running, ..Self::default() }
    }

    pub(crate) fn freed(path: PathBuf, freed: &Freed, running: Vec<String>) -> Self {
        Self {
            path,
            check: false,
            bytes: freed.bytes(),
            folders: freed.folders.iter().map(BuildFolder::of).collect(),
            kept: freed.kept.iter().map(|path| path.display().to_string()).collect(),
            failed: freed
                .failed
                .iter()
                .map(|(path, error)| FailedFolder { path: path.display().to_string(), error: error.clone() })
                .collect(),
            running,
        }
    }
}
