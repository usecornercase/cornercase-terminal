use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{Error, Result};
use crate::git::worktree::{check, git, lock};
use crate::process;
use crate::process::owners::{Owner, Trees, folder, owner};

use super::Scan;
use super::discover::{branch_at, gitdir_of};

pub(crate) fn scan(path: &Path, base: Option<&str>) -> Result<Scan> {
    if gitdir_of(path).is_none() {
        return Err(Error::Git(format!("{} is no longer a worktree", path.display())));
    }
    let committed = check(git(path, ["log", "-1", "--format=%ct", "HEAD"])?)?;
    let committed = String::from_utf8_lossy(&committed).trim().parse::<u64>().ok();
    let mut last_changed = committed.map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
    let status = check(git(
        path,
        [
            "--no-optional-locks",
            "-c",
            "diff.autoRefreshIndex=false",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
        ],
    )?)?;
    let paths = changed_paths(&status);
    for entry in &paths {
        let modified = std::fs::symlink_metadata(path.join(entry)).and_then(|meta| meta.modified()).ok();
        last_changed = last_changed.max(modified);
    }
    let base = base.filter(|base| resolves(path, base));
    let branch = branch_at(path);
    let (has_upstream, upstream_gone) = upstream(path, branch.as_deref())?;
    Ok(Scan {
        last_changed,
        changed: paths.len(),
        unpushed: unpushed(path, "HEAD", base)?,
        upstream_gone,
        merged: base.is_some_and(|base| is_ancestor(path, base)),
        has_upstream,
        branch,
        lock: lock(path)?,
    })
}

pub(crate) fn branch_commits(repo: &Path, branch: &str, base: Option<&str>) -> Result<Scan> {
    if branch.is_empty() || branch.starts_with('-') {
        return Err(Error::Git(format!("{branch:?} is not a branch")));
    }
    let reference = format!("refs/heads/{branch}");
    let base = base.filter(|base| resolves(repo, base));
    let (has_upstream, upstream_gone) = upstream(repo, Some(branch))?;
    let unpushed = if resolves(repo, &reference) { unpushed(repo, &reference, base)? } else { 0 };
    Ok(Scan {
        last_changed: None,
        changed: 0,
        unpushed,
        upstream_gone,
        merged: false,
        has_upstream,
        branch: Some(branch.to_string()),
        lock: None,
    })
}

fn changed_paths(status: &[u8]) -> Vec<String> {
    let mut records = status.split(|byte| *byte == 0).map(String::from_utf8_lossy);
    let mut paths = Vec::new();
    while let Some(record) = records.next() {
        let (fields, rename) = match record.chars().next() {
            Some('1') => (9, false),
            Some('2') => (10, true),
            Some('u') => (11, false),
            Some('?') => (2, false),
            _ => continue,
        };
        if let Some(path) = record.splitn(fields, ' ').nth(fields - 1) {
            paths.push(path.to_string());
        }
        if rename {
            records.next();
        }
    }
    paths
}

fn resolves(path: &Path, base: &str) -> bool {
    let spec = format!("{base}^{{commit}}");
    git(path, ["rev-parse", "--verify", "--quiet", spec.as_str()]).is_ok_and(|out| out.status.success())
}

fn unpushed(path: &Path, head: &str, base: Option<&str>) -> Result<usize> {
    let mut args = vec!["rev-list", "--count", head, "--not", "--remotes"];
    args.extend(base);
    let out = check(git(path, args)?)?;
    String::from_utf8_lossy(&out).trim().parse().map_err(|_| Error::Git("git rev-list printed no count".into()))
}

fn upstream(path: &Path, branch: Option<&str>) -> Result<(bool, bool)> {
    let Some(branch) = branch.filter(|b| !b.is_empty()) else { return Ok((false, false)) };
    let reference = format!("refs/heads/{branch}");
    let out = check(git(path, ["for-each-ref", "--format=%(upstream)%09%(upstream:track)", reference.as_str()])?)?;
    let text = String::from_utf8_lossy(&out);
    let (upstream, track) = text.trim_end().split_once('\t').unwrap_or((text.trim_end(), ""));
    Ok((upstream.starts_with("refs/remotes/"), track.contains("[gone]")))
}

fn is_ancestor(path: &Path, base: &str) -> bool {
    git(path, ["merge-base", "--is-ancestor", "HEAD", base]).is_ok_and(|out| out.status.success())
}

pub(crate) type Panes = Trees<Vec<usize>>;

pub(crate) fn outside(paths: &[PathBuf], panes: &Panes, server: i32) -> Vec<(usize, String, bool)> {
    let mut found = Vec::new();
    for pid in process::all() {
        if pid == server {
            continue;
        }
        let Some(cwd) = process::cwd(pid) else { continue };
        let Some(index) = folder(&cwd, paths) else { continue };
        let tab = match owner(pid, panes, server) {
            Owner::Pane(rows) if rows.contains(&index) => continue,
            Owner::Pane(_) => true,
            Owner::Server => continue,
            Owner::None => false,
        };
        if let Some(name) = process::name(pid) {
            found.push((index, name, tab));
        }
    }
    found
}
