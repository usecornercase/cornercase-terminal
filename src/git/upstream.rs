use std::path::{Path, PathBuf};

use crate::git::worktree;

pub(crate) fn check(repo: &Path, workspaces: &[(u64, PathBuf)], fetch: bool) -> Vec<(u64, u32)> {
    if fetch {
        let _ = worktree::git(repo, ["fetch", "--all", "--quiet"]);
    }
    workspaces.iter().map(|(id, path)| (*id, behind(path).unwrap_or(0))).collect()
}

pub(crate) fn behind(dir: &Path) -> Option<u32> {
    let out = worktree::git(dir, ["rev-list", "--count", "HEAD..@{upstream}"]).ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests;
