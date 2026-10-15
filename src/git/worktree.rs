use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use crate::error::{Error, Result};
use crate::log;
use crate::process;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pin {
    pub(crate) remote: String,
    pub(crate) reference: String,
    pub(crate) commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Made {
    Local,
    Tracking(String),
    New,
    Fetched(String),
}

impl Made {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Tracking(_) => "remote",
            Self::New => "new",
            Self::Fetched(_) => "fetched",
        }
    }

    pub(crate) fn from(&self) -> &str {
        match self {
            Self::Local => "",
            Self::Tracking(from) | Self::Fetched(from) => from,
            Self::New => "HEAD",
        }
    }
}

pub(crate) fn create(repo: &Path, branch: &str, path: &Path) -> Result<Made> {
    refuse_option("branch", branch)?;
    prepare(path)?;
    let made = if ref_exists(repo, &format!("refs/heads/{branch}"))? {
        Made::Local
    } else if let Some(remote) = remote_with(repo, branch)? {
        Made::Tracking(format!("{remote}/{branch}"))
    } else {
        Made::New
    };
    let path = path.as_os_str();
    let worktree_add = [OsStr::new("worktree"), "add".as_ref()];
    match &made {
        Made::Tracking(from) => {
            let args = ["--track".as_ref(), "-b".as_ref(), branch.as_ref(), path, from.as_ref()];
            check(git(repo, worktree_add.into_iter().chain(args))?)?;
        }
        Made::New => {
            let args = ["-b".as_ref(), branch.as_ref(), path, "HEAD".as_ref()];
            check(git(repo, worktree_add.into_iter().chain(args))?)?;
        }
        Made::Local | Made::Fetched(_) => {
            check(git(repo, worktree_add.into_iter().chain([path, branch.as_ref()]))?)?;
        }
    }
    copy_included(repo, Path::new(path))?;
    Ok(made)
}

pub(crate) fn create_pinned(repo: &Path, branch: &str, path: &Path, pin: &Pin) -> Result<Made> {
    refuse_option("branch", branch)?;
    refuse_option("remote", &pin.remote)?;
    check_reference(&pin.reference)?;
    let commit = commit_prefix(&pin.commit)?;
    prepare(path)?;
    let local = format!("refs/heads/{branch}");
    let made = if ref_exists(repo, &local)? {
        let at = commit_of(repo, &local)?;
        if !at.starts_with(&commit) {
            return Err(Error::Git(format!("branch {branch} already exists at {at}, not {commit}")));
        }
        Made::Local
    } else {
        let refspec = format!("{}:{local}", pin.reference);
        check(git(repo, ["fetch", "--no-tags", "--quiet", &pin.remote, &refspec])?)?;
        let at = commit_of(repo, &local)?;
        if !at.starts_with(&commit) {
            let _ = git(repo, ["branch", "-D", branch]);
            return Err(Error::Git(format!(
                "{} on {} is at {at}, not {commit}; nothing was checked out",
                pin.reference, pin.remote
            )));
        }
        Made::Fetched(format!("{} {}", pin.remote, pin.reference))
    };
    check(git(repo, [OsStr::new("worktree"), "add".as_ref(), path.as_os_str(), branch.as_ref()])?)?;
    copy_included(repo, path)?;
    Ok(made)
}

fn prepare(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(Error::PathExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn copy_included(repo: &Path, path: &Path) -> Result<()> {
    for file in included_files(repo)? {
        if let Err(e) = copy(&repo.join(&file), &path.join(&file)) {
            log::warning!("worktree", "failed to copy a file into the worktree", file = file.display(), error = e);
        }
    }
    Ok(())
}

fn refuse_option(what: &str, value: &str) -> Result<()> {
    if value.starts_with('-') {
        return Err(Error::Git(format!("the {what} `{value}` starts with `-`, which git would read as an option")));
    }
    Ok(())
}

fn check_reference(reference: &str) -> Result<()> {
    refuse_option("ref", reference)?;
    let bad = |c: char| c.is_whitespace() || c.is_control() || matches!(c, ':' | '*' | '^' | '~' | '?' | '[' | '\\');
    if reference.is_empty() || reference.starts_with('+') || reference.contains(bad) {
        return Err(Error::Git(format!("`{reference}` is not a ref to fetch")));
    }
    Ok(())
}

fn commit_prefix(commit: &str) -> Result<String> {
    let commit = commit.trim().to_ascii_lowercase();
    if (7..=64).contains(&commit.len()) && commit.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(commit)
    } else {
        Err(Error::Git(format!("`{commit}` is not a commit: give at least 7 hexadecimal digits of its hash")))
    }
}

fn commit_of(repo: &Path, reference: &str) -> Result<String> {
    let out = check(git(repo, ["rev-parse", "--verify", "--quiet", &format!("{reference}^{{commit}}")])?)?;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

fn remote_with(repo: &Path, branch: &str) -> Result<Option<String>> {
    let out = check(git(repo, ["remote"])?)?;
    let mut remotes = Vec::new();
    for remote in String::from_utf8_lossy(&out).lines().map(str::trim).filter(|r| !r.is_empty()) {
        if ref_exists(repo, &format!("refs/remotes/{remote}/{branch}"))? {
            remotes.push(remote.to_string());
        }
    }
    if remotes.len() < 2 {
        return Ok(remotes.pop());
    }
    let default = git(repo, ["config", "--get", "checkout.defaultRemote"])?;
    let default = String::from_utf8_lossy(&default.stdout).trim().to_string();
    if remotes.contains(&default) {
        return Ok(Some(default));
    }
    Err(Error::Git(format!(
        "branch {branch} is on several remotes ({}); set checkout.defaultRemote to the one to track",
        remotes.join(", ")
    )))
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

fn ref_exists(repo: &Path, reference: &str) -> Result<bool> {
    let out = git(repo, ["show-ref", "--verify", "--quiet", reference])?;
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
