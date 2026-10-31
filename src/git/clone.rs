use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::worktree::git_env;

const NOT_A_REPOSITORY: [&str; 4] =
    ["does not exist", "not found", "could not be found", "does not appear to be a git repository"];
const DENIED: [&str; 7] = [
    "permission denied",
    "authentication failed",
    "could not read username",
    "could not read password",
    "access denied",
    "returned error: 401",
    "returned error: 403",
];
const PREFIXES: [&str; 4] = ["fatal: ", "error: ", "remote: ", "ERROR: "];
const SKIPPED: [&str; 2] = ["hint:", "warning:"];
const ATTEMPTS: u32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) url: String,
    pub(crate) path: PathBuf,
    made: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    NoName,
    GitFolder,
    NotEmpty(PathBuf),
    Folder(String),
    NotRepository(String),
    AccessDenied(String),
    Failed(String),
}

impl Failure {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::NoName => "no name",
            Self::GitFolder => "named .git",
            Self::NotEmpty(_) => "not empty",
            Self::Folder(_) => "no folder",
            Self::NotRepository(_) => "not a repository",
            Self::AccessDenied(_) => "access denied",
            Self::Failed(_) => "git failed",
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoName => write!(f, "cannot tell the folder's name from the URL"),
            Self::GitFolder => write!(f, "the URL names the repository `.git`, which cannot be a clone's folder"),
            Self::NotEmpty(path) => write!(f, "`{}` already exists and is not an empty folder", path.display()),
            Self::Folder(detail) => write!(f, "cannot make the folder to clone into: {detail}"),
            Self::NotRepository(detail) => write!(f, "the URL is not a git repository: {detail}"),
            Self::AccessDenied(detail) => write!(f, "access to the repository was denied: {detail}"),
            Self::Failed(detail) => write!(f, "git clone failed: {detail}"),
        }
    }
}

pub(crate) fn is_local_path(url: &str) -> bool {
    if url.contains("://") {
        return false;
    }
    match (url.find(':'), url.find('/')) {
        (Some(colon), Some(slash)) => slash < colon,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn naming_part(url: &str) -> String {
    if is_local_path(url) {
        return url.to_string();
    }
    match url.split_once("://") {
        Some((_, rest)) => bare(rest),
        None => url.split(['?', '#']).next().unwrap_or(url).to_string(),
    }
}

fn bare(url: &str) -> String {
    let before_path = &url[..url.find('/').unwrap_or(url.len())];
    let rest = &url[before_path.rfind('@').map_or(0, |at| at + 1)..];
    rest.split(['?', '#']).next().unwrap_or(rest).to_string()
}

fn folder_name(url: &str) -> Result<String, Failure> {
    let url = naming_part(url.trim());
    let url = url.trim_end_matches('/');
    let url = url.strip_suffix("/.git").unwrap_or(url).trim_end_matches('/');
    let last = url.rsplit(['/', ':']).next().unwrap_or_default();
    let name = last.strip_suffix(".git").or_else(|| last.strip_suffix(".bundle")).unwrap_or(last);
    match name {
        "" | "." | ".." => Err(Failure::NoName),
        name if name.eq_ignore_ascii_case(".git") => Err(Failure::GitFolder),
        name => Ok(name.to_string()),
    }
}

pub(crate) fn destination(url: &str, parent: &Path) -> Result<PathBuf, Failure> {
    let name = folder_name(url)?;
    let found = parent.ancestors().find_map(|at| at.canonicalize().ok().map(|real| (at, real)));
    let parent = match found {
        Some((at, real)) => real.join(parent.strip_prefix(at).unwrap_or(Path::new(""))),
        None => parent.to_path_buf(),
    };
    let path = parent.join(name);
    Ok(path.canonicalize().unwrap_or(path))
}

pub(crate) fn prepare(url: &str, path: PathBuf) -> Result<Target, Failure> {
    prepare_with(url, path, |dir| std::fs::create_dir(dir))
}

fn prepare_with(url: &str, path: PathBuf, create: impl FnMut(&Path) -> io::Result<()>) -> Result<Target, Failure> {
    let mut target = Target { url: url.to_string(), path, made: Vec::new() };
    match target.make(create) {
        Ok(()) => Ok(target),
        Err(failure) => {
            target.undo();
            Err(failure)
        }
    }
}

pub(crate) fn run(target: &Target) -> Result<(), Failure> {
    let args = [OsStr::new("clone"), "--quiet".as_ref(), "--".as_ref(), target.url.as_ref(), target.path.as_ref()];
    let failure = match git_env(&target.path, args, &[("GCM_INTERACTIVE", OsStr::new("never"))]) {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => failure(&String::from_utf8_lossy(&out.stderr)),
        Err(e) => Failure::Failed(e.to_string()),
    };
    target.undo();
    Err(failure)
}

impl Target {
    fn make(&mut self, mut create: impl FnMut(&Path) -> io::Result<()>) -> Result<(), Failure> {
        let mut attempt = 1;
        let created = loop {
            match self.make_once(&mut create) {
                Ok(created) => break created,
                Err((_, e)) if e.kind() == io::ErrorKind::NotFound && attempt < ATTEMPTS => attempt += 1,
                Err((dir, e)) => return Err(Failure::Folder(format!("`{}`: {e}", dir.display()))),
            }
        };
        if !created && !std::fs::read_dir(&self.path).is_ok_and(|mut entries| entries.next().is_none()) {
            return Err(Failure::NotEmpty(self.path.clone()));
        }
        Ok(())
    }

    fn make_once(&mut self, create: &mut impl FnMut(&Path) -> io::Result<()>) -> Result<bool, (PathBuf, io::Error)> {
        let missing = self.path.ancestors().skip(1).take_while(|at| std::fs::symlink_metadata(at).is_err());
        let missing: Vec<PathBuf> = missing.map(Path::to_path_buf).collect();
        for dir in missing.into_iter().rev() {
            if made(&dir, create)? {
                self.made.push(dir);
            }
        }
        let created = made(&self.path, create)?;
        if created {
            self.made.push(self.path.clone());
        }
        Ok(created)
    }

    fn undo(&self) {
        for dir in self.made.iter().rev() {
            let _ = if *dir == self.path { std::fs::remove_dir_all(dir) } else { std::fs::remove_dir(dir) };
        }
    }
}

fn made(dir: &Path, create: &mut impl FnMut(&Path) -> io::Result<()>) -> Result<bool, (PathBuf, io::Error)> {
    match create(dir) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err((dir.to_path_buf(), e)),
    }
}

fn failure(stderr: &str) -> Failure {
    let kept = stderr.lines().map(str::trim).filter(|line| !SKIPPED.iter().any(|skip| line.starts_with(skip)));
    let lines: Vec<String> = kept.map(said).filter(|line| !line.is_empty()).collect();
    let with =
        |marks: &[&str]| lines.iter().find(|line| marks.iter().any(|mark| line.to_lowercase().contains(mark))).cloned();
    if let Some(detail) = with(&NOT_A_REPOSITORY) {
        return Failure::NotRepository(detail);
    }
    if let Some(detail) = with(&DENIED) {
        return Failure::AccessDenied(detail);
    }
    Failure::Failed(lines.first().cloned().unwrap_or_else(|| "git gave no reason".to_string()))
}

fn said(line: &str) -> String {
    let line = PREFIXES.iter().find_map(|prefix| line.strip_prefix(prefix)).unwrap_or(line);
    redacted(line.trim_end_matches('.'))
}

fn redacted(line: &str) -> String {
    let mut text = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        let end = tail.find([' ', '\'', '"']).unwrap_or(tail.len());
        text.push_str(head);
        text.push_str(&bare(&tail[..end]));
        rest = &tail[end..];
    }
    text.push_str(rest);
    text
}

#[cfg(test)]
mod tests;
