pub(crate) mod upstream;
pub(crate) mod worktree;

use std::path::{Path, PathBuf};

const SHORT_SHA_LEN: usize = 7;

pub(crate) fn branch(dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(dir)?.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        return Some(reference.strip_prefix("refs/heads/").unwrap_or(reference).to_string());
    }
    head.get(..SHORT_SHA_LEN).map(str::to_string)
}

#[cfg(test)]
pub(crate) fn main_worktree(dir: &Path) -> Option<PathBuf> {
    let root = dir.ancestors().find(|d| d.join(".git").exists())?;
    root.join(".git").is_dir().then(|| root.to_path_buf())
}

pub(crate) fn is_repo_root(dir: &Path) -> bool {
    dir.join(".git").is_dir()
}

pub(crate) fn linked_worktrees(repo: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(repo.join(".git").join("worktrees")) else { return Vec::new() };
    let mut worktrees: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("gitdir")).ok())
        .filter_map(|gitdir| Path::new(gitdir.trim()).parent().map(Path::to_path_buf))
        .filter(|path| path.is_dir())
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect();
    worktrees.sort();
    worktrees
}

fn git_dir(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find_map(|d| {
        let dot_git = d.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        let link = std::fs::read_to_string(&dot_git).ok()?;
        let target = link.trim().strip_prefix("gitdir:")?.trim();
        Some(d.join(target))
    })
}

#[cfg(test)]
mod tests;
