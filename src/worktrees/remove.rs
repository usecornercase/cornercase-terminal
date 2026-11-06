use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::git;
use crate::git::worktree::{RemoveOptions, check, git as run};
use crate::log;
use crate::os::ffi;
use crate::os::fs::{canonical, canonicalize};

use super::discover::{Origin, plain, repo_of};
use super::scan::branch_tip;
use super::{Found, Kind};

pub(crate) const ALL: RemoveOptions = RemoveOptions { force: true, unlock: true };

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Named {
    Listed(PathBuf),
    Unknown { path: PathBuf, why: String },
}

pub(crate) fn named(asked: &Path, open: &[PathBuf], found: &[Found]) -> Named {
    if !asked.is_absolute() {
        return Named::Unknown { path: asked.to_path_buf(), why: "it is not an absolute path".to_string() };
    }
    let place = real_place(asked);
    let listed = |row: &&Found| ffi::same(&row.path, &place) || ffi::same(&real_place(&row.path), &place);
    let Some(row) = found.iter().find(listed) else {
        return Named::Unknown { why: unknown_why(&place, open), path: place };
    };
    match recorded_at_a_link(row) {
        Some(link) => {
            let why = format!("git records it at the symbolic link {} and would delete through it", link.display());
            Named::Unknown { path: row.path.clone(), why }
        }
        None => Named::Listed(row.path.clone()),
    }
}

fn recorded_at_a_link(found: &Found) -> Option<PathBuf> {
    let admin = found.admin.as_deref().filter(|name| plain(name))?;
    let entry = found.repo.join(".git").join("worktrees").join(admin);
    let gitdir = std::fs::read_to_string(entry.join("gitdir")).ok()?;
    let folder = ffi::join(&entry, gitdir.trim()).parent()?.to_path_buf();
    std::fs::symlink_metadata(&folder).ok()?.file_type().is_symlink().then_some(folder)
}

fn real_place(path: &Path) -> PathBuf {
    if let Ok(real) = canonicalize(path) {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => real_place(parent).join(name),
        _ => canonical(path),
    }
}

fn unknown_why(place: &Path, open: &[PathBuf]) -> String {
    if std::fs::symlink_metadata(place.join(".git")).is_ok_and(|dot_git| dot_git.is_dir()) {
        return "it is a repository's main checkout, which is never removed".to_string();
    }
    if open.iter().any(|folder| ffi::same(&canonical(folder), place)) {
        return "it is a project's own folder".to_string();
    }
    if std::fs::symlink_metadata(place).is_err() {
        return "nothing is there, and no repository lists a worktree there".to_string();
    }
    "it is not a worktree that `cornercase worktrees list` knows".to_string()
}

pub(crate) fn still_listed(path: &Path) -> Result<()> {
    let changed = || Error::Git(format!("{} changed since it was listed", path.display()));
    match (std::fs::symlink_metadata(path), canonicalize(path)) {
        (Ok(meta), _) if meta.file_type().is_symlink() => Err(changed()),
        (Ok(_), Ok(real)) if !ffi::same(&real, path) => Err(changed()),
        _ => Ok(()),
    }
}

pub(crate) fn remove(found: &Found, script: Option<&str>, options: RemoveOptions) -> Result<Option<String>> {
    still_listed(&found.path)?;
    match found.kind {
        Kind::Linked => {
            let branch = found.branch.as_deref().unwrap_or("");
            git::worktree::remove_with_teardown(&found.repo, &found.path, branch, script, options)
        }
        Kind::Prunable => prune(found).map(|()| None),
        Kind::Orphan => orphan(&found.path).map(|()| None),
        Kind::Unknown => unknown(&found.path).map(|()| None),
    }
}

pub(crate) fn delete_branch(repo: &Path, branch: &str) -> Result<()> {
    if branch.is_empty() || branch.starts_with('-') {
        return Err(Error::Git(format!("refusing to delete the branch {branch:?}")));
    }
    let default = crate::changes::git::default_base(repo);
    let default = default.as_deref().map(|name| name.strip_prefix("origin/").unwrap_or(name));
    if default == Some(branch) || git::checked_out(repo).as_deref() == Some(branch) {
        return Err(Error::Git(format!("refusing to delete the branch {branch}, the main checkout's or the default")));
    }
    check(run(repo, ["branch", "-D", branch])?).map(drop)?;
    log::info!("worktrees", "branch deleted", repo = repo.display(), branch = branch);
    Ok(())
}

pub(crate) fn delete_branch_at(repo: &Path, branch: &str, tip: Option<&str>) -> Result<bool> {
    let moved = tip.is_some_and(|tip| branch_tip(repo, branch).is_some_and(|now| now != tip));
    if moved {
        log::info!("worktrees", "branch kept: it moved since it was looked at", repo = repo.display(), branch = branch);
        return Ok(false);
    }
    delete_branch(repo, branch).map(|()| true)
}

fn prune(found: &Found) -> Result<()> {
    let Some(admin) = found.admin.as_deref().filter(|name| plain(name)) else {
        return Err(Error::Git("this entry has no folder of its own to prune".to_string()));
    };
    let out = check(run(&found.repo, ["worktree", "list", "--porcelain"])?)?;
    let still = super::discover::parse_porcelain(&String::from_utf8_lossy(&out))
        .into_iter()
        .any(|(path, _, prunable)| prunable && same(&path, &found.path));
    if !still {
        return Err(Error::Git(format!("{} is no longer prunable", found.path.display())));
    }
    let common = check(run(&found.repo, ["rev-parse", "--path-format=absolute", "--git-common-dir"])?)?;
    let common = std::str::from_utf8(common.trim_ascii_end()).map_err(|e| Error::Git(e.to_string()))?;
    let dir = ffi::to_path(common.as_bytes()).join("worktrees").join(admin);
    real_dir(&dir)?;
    std::fs::remove_dir_all(&dir)?;
    log::info!("worktrees", "pruned", path = found.path.display());
    Ok(())
}

fn same(a: &Path, b: &Path) -> bool {
    ffi::same(a, b) || a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b)
}

fn real_dir(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() { Ok(()) } else { Err(Error::Git(format!("{} is not a folder", path.display()))) }
}

fn orphan(path: &Path) -> Result<()> {
    real_dir(path)?;
    let dot_git = std::fs::symlink_metadata(path.join(".git"))?;
    if !dot_git.is_file() {
        return Err(Error::Git(format!("{} is not a worktree folder", path.display())));
    }
    let text = std::fs::read_to_string(path.join(".git"))?;
    let target = text.trim().strip_prefix("gitdir:").map(str::trim);
    let Some(target) = target else {
        return Err(Error::Git(format!("{} is not a worktree folder", path.display())));
    };
    let looked = std::fs::symlink_metadata(ffi::join(path, target));
    let gone = matches!(looked, Err(e) if e.kind() == std::io::ErrorKind::NotFound);
    if !gone {
        return Err(Error::Git(format!("the repository of {} is back", path.display())));
    }
    std::fs::remove_dir_all(path)?;
    log::info!("worktrees", "orphan folder deleted", path = path.display());
    Ok(())
}

fn unknown(path: &Path) -> Result<()> {
    real_dir(path)?;
    if !std::fs::symlink_metadata(path.join(".git"))?.is_file() {
        return Err(Error::Git(format!("{} is not a worktree folder", path.display())));
    }
    match repo_of(path) {
        Origin::Unknown(_) => {}
        Origin::Repo(_) => return Err(Error::Git(format!("git knows {} again", path.display()))),
        Origin::Gone(_) | Origin::None => {
            return Err(Error::Git(format!("{} changed since it was listed", path.display())));
        }
    }
    std::fs::remove_dir_all(path)?;
    log::info!("worktrees", "folder unknown to git deleted", path = path.display());
    Ok(())
}
