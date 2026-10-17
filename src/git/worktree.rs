use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::error::{Error, Result};
use crate::log;
use crate::process;

pub(crate) mod setup;

pub(crate) const INCLUDE_FILE: &str = ".worktreeinclude";
const DEFAULT_SLUG: &str = "worktree";

pub(crate) fn slug(branch: &str) -> String {
    let mut slug = String::new();
    for c in branch.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() { DEFAULT_SLUG.into() } else { slug.into() }
}

pub(crate) fn checkout_path(worktrees_dir: &Path, repo: &Path, branch: &str) -> PathBuf {
    let name = repo.file_name().unwrap_or(repo.as_os_str());
    worktrees_dir.join(name).join(slug(branch))
}

pub(crate) fn create(repo: &Path, branch: &str, path: &Path) -> Result<()> {
    create_from(repo, branch, path, "HEAD")
}

pub(crate) fn create_from(repo: &Path, branch: &str, path: &Path, base: &str) -> Result<()> {
    if path.exists() {
        return Err(Error::PathExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path = path.as_os_str();
    if branch_exists(repo, branch)? {
        check(git(repo, [OsStr::new("worktree"), "add".as_ref(), path, branch.as_ref()])?)?;
    } else {
        check(git(
            repo,
            [OsStr::new("worktree"), "add".as_ref(), "-b".as_ref(), branch.as_ref(), path, base.as_ref()],
        )?)?;
    }
    for file in included_files(repo)? {
        if let Err(e) = copy(&repo.join(&file), &Path::new(path).join(&file)) {
            log::warning!("worktree", "failed to copy a file into the worktree", file = file.display(), error = e);
        }
    }
    Ok(())
}

pub(crate) fn remove(repo: &Path, path: &Path, force: bool, unlock: bool) -> Result<()> {
    if unlock && !matches!(lock(path), Ok(None)) {
        check(git(repo, [OsStr::new("worktree"), "unlock".as_ref(), path.as_os_str()])?)?;
    }
    let mut args = vec![OsStr::new("worktree"), "remove".as_ref()];
    if force {
        args.push("--force".as_ref());
    }
    args.push(path.as_os_str());
    check(git(repo, args)?).map(drop)
}

pub(crate) fn changed(path: &Path) -> Result<bool> {
    check(git(path, ["--no-optional-locks", "status", "--porcelain"])?).map(|out| !out.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lock {
    pub(crate) reason: String,
    pub(crate) holder: Option<Holder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holder {
    Running(i32),
    Gone(i32),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) changed: bool,
    pub(crate) lock: Option<Lock>,
}

pub(crate) fn status(path: &Path) -> Status {
    Status { changed: !matches!(changed(path), Ok(false)), lock: lock(path).ok().flatten() }
}

pub(crate) fn lock(path: &Path) -> Result<Option<Lock>> {
    let out = check(git(path, ["rev-parse", "--absolute-git-dir"])?)?;
    let dir = Path::new(OsStr::from_bytes(out.trim_ascii_end())).join("locked");
    let reason = match std::fs::read_to_string(dir) {
        Ok(reason) => reason.trim().to_string(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let holder = pid_in(&reason).map(|pid| if process::alive(pid) { Holder::Running(pid) } else { Holder::Gone(pid) });
    Ok(Some(Lock { reason, holder }))
}

fn pid_in(reason: &str) -> Option<i32> {
    let (_, after) = reason.split_once("pid ")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub(crate) fn git<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(repo: &Path, args: I) -> Result<Output> {
    command(repo, args).stdin(Stdio::null()).output().map_err(Error::RunGit)
}

fn command<I: IntoIterator<Item = S>, S: AsRef<OsStr>>(repo: &Path, args: I) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "")
        .env("SSH_ASKPASS_REQUIRE", "never");
    cmd
}

pub(crate) fn check(out: Output) -> Result<Vec<u8>> {
    if out.status.success() {
        return Ok(out.stdout);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let fatal =
        lines.iter().rev().find_map(|line| line.strip_prefix("fatal: ").or_else(|| line.strip_prefix("error: ")));
    let message = fatal
        .or_else(|| lines.iter().rev().find(|line| !line.starts_with("hint:")).copied())
        .map_or_else(|| format!("git failed with {}", out.status), str::to_string);
    Err(Error::Git(message))
}

fn branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    let out = git(repo, ["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => check(out).map(|_| false),
    }
}

fn included_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let include = repo.join(INCLUDE_FILE);
    if !include.is_file() {
        return Ok(Vec::new());
    }
    let mut exclude_from = OsString::from("--exclude-from=");
    exclude_from.push(&include);
    let matching = check(git(
        repo,
        [OsStr::new("ls-files"), "--others".as_ref(), "--ignored".as_ref(), "-z".as_ref(), &exclude_from],
    )?)?;
    if matching.is_empty() {
        return Ok(Vec::new());
    }
    let ignored = ignored_by_git(repo, matching)?;
    Ok(ignored.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| PathBuf::from(OsStr::from_bytes(p))).collect())
}

fn ignored_by_git(repo: &Path, paths: Vec<u8>) -> Result<Vec<u8>> {
    let mut child = command(repo, ["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::RunGit)?;
    let mut stdin = child.stdin.take().ok_or_else(|| Error::Git("git check-ignore has no stdin".into()))?;
    let writer = std::thread::spawn(move || stdin.write_all(&paths));
    let out = child.wait_with_output().map_err(Error::RunGit)?;
    let _ = writer.join();
    if out.status.code() == Some(1) && out.stdout.is_empty() {
        return Ok(Vec::new());
    }
    check(out)
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::copy(from, to).map(drop)
}

#[cfg(test)]
mod tests;
