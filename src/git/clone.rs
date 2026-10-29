use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use super::worktree::git;

const NOT_A_REPOSITORY: [&str; 3] = ["does not exist", "not found", "does not appear to be a git repository"];
const DENIED: [&str; 6] = [
    "permission denied",
    "authentication failed",
    "could not read username",
    "could not read password",
    "access denied",
    "returned error: 403",
];
const PREFIXES: [&str; 4] = ["fatal: ", "error: ", "remote: ", "ERROR: "];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) url: String,
    pub(crate) path: PathBuf,
    made: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    NoName(String),
    NotEmpty(PathBuf),
    NotRepository { url: String, detail: String },
    AccessDenied { url: String, detail: String },
    Failed(String),
}

impl Failure {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::NoName(_) => "no name",
            Self::NotEmpty(_) => "not empty",
            Self::NotRepository { .. } => "not a repository",
            Self::AccessDenied { .. } => "access denied",
            Self::Failed(_) => "git failed",
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NoName(url) => write!(f, "cannot tell the folder's name from `{url}`"),
            Self::NotEmpty(path) => write!(f, "`{}` already exists and is not an empty folder", path.display()),
            Self::NotRepository { url, detail } => write!(f, "`{url}` is not a git repository: {detail}"),
            Self::AccessDenied { url, detail } => write!(f, "access denied to `{url}`: {detail}"),
            Self::Failed(detail) => write!(f, "git clone failed: {detail}"),
        }
    }
}

fn folder_name(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix("/.git").unwrap_or(url).trim_end_matches('/');
    let last = url.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").or_else(|| last.strip_suffix(".bundle")).unwrap_or(last);
    (!matches!(name, "" | "." | "..")).then(|| name.to_string())
}

pub(crate) fn target(url: &str, parent: &Path) -> Result<Target, Failure> {
    let name = folder_name(url).ok_or_else(|| Failure::NoName(url.to_string()))?;
    let path = parent.join(name);
    if !empty_or_missing(&path) {
        return Err(Failure::NotEmpty(path));
    }
    let made = path.ancestors().take_while(|at| std::fs::symlink_metadata(at).is_err()).last().map(Path::to_path_buf);
    Ok(Target { url: url.to_string(), path, made })
}

fn empty_or_missing(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => std::fs::symlink_metadata(path).is_err(),
    }
}

pub(crate) fn run(target: &Target) -> Result<(), Failure> {
    let at = target.path.ancestors().find(|at| at.is_dir()).unwrap_or(Path::new("/"));
    let args = [OsStr::new("clone"), "--quiet".as_ref(), "--".as_ref(), target.url.as_ref(), target.path.as_ref()];
    let failure = match git(at, args) {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => failure(&target.url, &String::from_utf8_lossy(&out.stderr)),
        Err(e) => Failure::Failed(e.to_string()),
    };
    target.undo();
    Err(failure)
}

impl Target {
    fn undo(&self) {
        let Some(made) = &self.made else { return };
        let _ = std::fs::remove_dir_all(&self.path);
        for dir in self.path.ancestors().skip(1).take_while(|dir| dir.starts_with(made)) {
            if std::fs::remove_dir(dir).is_err() {
                return;
            }
        }
    }
}

fn failure(url: &str, stderr: &str) -> Failure {
    let lines: Vec<&str> =
        stderr.lines().map(said).filter(|line| !line.is_empty() && !line.starts_with("hint:")).collect();
    let with = |marks: &[&str]| {
        let found = lines.iter().find(|line| marks.iter().any(|mark| line.to_lowercase().contains(mark)));
        found.map(|line| (*line).to_string())
    };
    let url = url.to_string();
    if let Some(detail) = with(&NOT_A_REPOSITORY) {
        return Failure::NotRepository { url, detail };
    }
    if let Some(detail) = with(&DENIED) {
        return Failure::AccessDenied { url, detail };
    }
    Failure::Failed(lines.first().map_or_else(|| "git gave no reason".to_string(), |line| (*line).to_string()))
}

fn said(line: &str) -> &str {
    let line = line.trim();
    let line = PREFIXES.iter().find_map(|prefix| line.strip_prefix(prefix)).unwrap_or(line);
    line.trim_end_matches('.')
}

#[cfg(test)]
mod tests;
