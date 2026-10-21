use std::path::Path;

use crate::error::{Error, Result};
use crate::git;
use crate::git::worktree::{check, git as run};
use crate::log;

use super::discover::{Origin, plain, repo_of};
use super::{Found, Kind};

const ALL: git::worktree::RemoveOptions = git::worktree::RemoveOptions { force: true, unlock: true };

pub(crate) fn remove(found: &Found, script: Option<&str>) -> Result<Option<String>> {
    match found.kind {
        Kind::Linked => {
            let branch = found.branch.as_deref().unwrap_or("");
            git::worktree::remove_with_teardown(&found.repo, &found.path, branch, script, ALL)
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
    let common = Path::new(std::str::from_utf8(common.trim_ascii_end()).map_err(|e| Error::Git(e.to_string()))?);
    let dir = common.join("worktrees").join(admin);
    real_dir(&dir)?;
    std::fs::remove_dir_all(&dir)?;
    log::info!("worktrees", "pruned", path = found.path.display());
    Ok(())
}

fn same(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(a, b)| a == b)
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
    let gone =
        matches!(std::fs::symlink_metadata(path.join(target)), Err(e) if e.kind() == std::io::ErrorKind::NotFound);
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
