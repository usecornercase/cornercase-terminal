use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Fields;
use super::arrange::Locked;
use crate::time::Stamp;
use crate::worktrees::builds::{Folder, Freed, Size};
use crate::worktrees::{Kind, Row};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ListWorktrees {
    pub(crate) project: Option<u64>,
    pub(crate) sizes: bool,
}

impl Default for ListWorktrees {
    fn default() -> Self {
        Self { project: None, sizes: true }
    }
}

impl ListWorktrees {
    pub(super) fn fields(&self, fields: &mut Fields) {
        fields.maybe("project", self.project);
        fields.add("sizes", self.sizes);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WorktreeListing {
    pub(crate) worktrees: Vec<WorktreeInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own fact about the worktree")]
pub(crate) struct WorktreeInfo {
    pub(crate) path: PathBuf,
    pub(crate) kind: String,
    pub(crate) project_name: String,
    pub(crate) repository: Option<PathBuf>,
    pub(crate) branch: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) last_changed: Option<String>,
    pub(crate) changed: Option<usize>,
    pub(crate) unpushed: Option<usize>,
    pub(crate) merged: bool,
    pub(crate) no_commits: bool,
    pub(crate) upstream_gone: bool,
    pub(crate) lock: Option<Locked>,
    pub(crate) running: Vec<String>,
    pub(crate) outside: Vec<String>,
    pub(crate) other_tabs: Vec<String>,
    pub(crate) removing: bool,
    pub(crate) run_role: bool,
    pub(crate) safe: bool,
    pub(crate) risks: Vec<String>,
    pub(crate) error: Option<String>,
    pub(crate) size: Option<WorktreeSized>,
    pub(crate) size_error: Option<String>,
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Linked => "linked",
        Kind::Prunable => "prunable",
        Kind::Orphan => "repository-gone",
        Kind::Unknown => "unknown-to-git",
    }
}

impl WorktreeInfo {
    pub(crate) fn of(row: &Row, risks: Vec<String>) -> Self {
        let gone = matches!(row.found.kind, Kind::Orphan | Kind::Unknown);
        let live = row.live.as_ref();
        let mut info = Self {
            path: row.found.path.clone(),
            kind: kind_name(row.found.kind).to_string(),
            project_name: row.found.project.clone(),
            repository: (!gone).then(|| row.found.repo.clone()),
            branch: row.branch().map(str::to_string),
            project: live.map(|live| live.project),
            workspace: live.map(|live| live.workspace),
            merged: row.merged(),
            no_commits: row.no_commits(),
            running: live.map(|live| live.running.clone()).unwrap_or_default(),
            outside: row.outside.clone(),
            other_tabs: row.elsewhere.clone(),
            removing: row.removing || live.is_some_and(|live| live.removing),
            run_role: live.is_some_and(|live| live.role_of_run),
            safe: !row.risky(),
            risks,
            ..Self::default()
        };
        info.scanned(row);
        info.sized(row);
        info
    }

    fn scanned(&mut self, row: &Row) {
        match &row.scan {
            Some(Ok(scan)) => {
                self.last_changed = scan.last_changed.map(|at| Stamp(at).to_string());
                self.changed = (row.found.kind == Kind::Linked).then_some(scan.changed);
                self.unpushed = Some(scan.unpushed);
                self.upstream_gone = scan.upstream_gone;
                self.lock = scan.lock.as_ref().map(|lock| Locked::of(lock, self.path.clone()));
            }
            Some(Err(error)) => self.error = Some(error.clone()),
            None => {}
        }
    }

    fn sized(&mut self, row: &Row) {
        match &row.size {
            Some(Ok(size)) => self.size = Some(WorktreeSized::of(self.path.clone(), size)),
            Some(Err(error)) => self.size_error = Some(error.clone()),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests;
