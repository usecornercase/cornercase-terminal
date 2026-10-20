use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use super::Mode;
use super::diff::{self, Diff, File, MAX_UNTRACKED_BYTES};
use crate::error::{Error, Result};
use crate::git::worktree;

const GLOBAL: [&str; 5] = ["--no-optional-locks", "-c", "core.quotepath=false", "-c", "diff.autoRefreshIndex=false"];
const DIFF: [&str; 9] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--find-renames",
    "--relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
    "--submodule=short",
];
const DEFAULT_BRANCHES: [&str; 2] = ["main", "master"];
const UNTRACKED_READ_BYTES: u64 = 8 << 20;
const REMOTE_HEAD: &str = "refs/remotes/origin/HEAD";

#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub(crate) dir: PathBuf,
    pub(crate) mode: Mode,
    pub(crate) base: Option<String>,
    pub(crate) last: Option<u64>,
    pub(crate) previous: Vec<Arc<File>>,
    pub(crate) expanded: Option<Vec<String>>,
}

#[derive(Debug)]
pub(crate) struct Loaded {
    pub(crate) base: Option<String>,
    pub(crate) digest: u64,
    pub(crate) diff: Option<Diff>,
}

pub(super) fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    worktree::check(worktree::git(dir, GLOBAL.iter().chain(args))?)
}

fn text(dir: &Path, args: &[&str]) -> Option<String> {
    let out = run(dir, args).ok()?;
    let text = String::from_utf8_lossy(&out).trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub(crate) fn default_base(dir: &Path) -> Option<String> {
    if let Some(head) = text(dir, &["symbolic-ref", "--quiet", REMOTE_HEAD]) {
        return Some(head.strip_prefix("refs/remotes/").unwrap_or(&head).to_string());
    }
    DEFAULT_BRANCHES
        .into_iter()
        .find(|name| text(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_some())
        .map(str::to_string)
}

pub(crate) fn branches(dir: &Path) -> Vec<String> {
    let Some(refs) = text(dir, &["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes"]) else {
        return Vec::new();
    };
    refs.lines()
        .filter(|r| !r.ends_with("/HEAD"))
        .filter_map(|r| r.strip_prefix("refs/heads/").or_else(|| r.strip_prefix("refs/remotes/")))
        .map(str::to_string)
        .collect()
}

pub(super) fn head(dir: &Path) -> Result<String> {
    text(dir, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .or_else(|| text(dir, &["hash-object", "-t", "tree", "/dev/null"]))
        .ok_or_else(|| Error::Git("not a git repository".into()))
}

fn merge_base(dir: &Path, base: &str) -> Result<String> {
    text(dir, &["merge-base", "HEAD", base]).ok_or_else(|| Error::Git(format!("cannot compare with {base}")))
}

#[derive(Hash)]
pub(super) struct Untracked {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
}

pub(super) fn untracked(dir: &Path) -> Result<Vec<Untracked>> {
    let out = run(dir, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let paths = out.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|raw| String::from_utf8_lossy(raw).into_owned());
    Ok(read_untracked(dir, paths))
}

pub(super) fn read_untracked(dir: &Path, paths: impl IntoIterator<Item = String>) -> Vec<Untracked> {
    let mut files = Vec::new();
    let mut budget = UNTRACKED_READ_BYTES;
    for path in paths {
        let Ok(meta) = std::fs::symlink_metadata(dir.join(&path)) else { continue };
        if meta.is_symlink() {
            let target = std::fs::read_link(dir.join(&path)).map(|t| t.into_os_string().into_vec()).unwrap_or_default();
            let size = target.len() as u64;
            files.push(Untracked { path, size, bytes: Some(target), modified: meta.modified().ok() });
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let fits = meta.len() <= MAX_UNTRACKED_BYTES && meta.len() <= budget;
        let bytes = fits.then(|| read(&dir.join(&path))).flatten();
        budget = budget.saturating_sub(bytes.as_ref().map_or(0, |b| b.len() as u64));
        files.push(Untracked { path, size: meta.len(), bytes, modified: meta.modified().ok() });
    }
    files
}

fn read(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(MAX_UNTRACKED_BYTES + 1).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn find(request: &Request, path: &str) -> Result<Option<Arc<File>>> {
    let loaded = load(request)?;
    Ok(loaded.diff.and_then(|diff| diff.files.into_iter().find(|f| f.path == path)))
}

fn single(dir: &Path, mode: Mode, base: Option<String>, expanded: Option<Vec<String>>) -> Request {
    Request { dir: dir.to_path_buf(), mode, base, last: None, previous: Vec::new(), expanded }
}

pub(crate) fn read_file(dir: &Path, mode: Mode, base: Option<String>, path: &str) -> Result<Option<Arc<File>>> {
    let expanded = (mode == Mode::Uncommitted).then(|| vec![path.to_string()]);
    find(&single(dir, mode, base, expanded), path)
}

pub(crate) fn listed(dir: &Path, path: &str) -> Result<Option<Arc<File>>> {
    find(&single(dir, Mode::Uncommitted, None, Some(Vec::new())), path)
}

pub(crate) fn load(request: &Request) -> Result<Loaded> {
    if let Some(expanded) = &request.expanded {
        return super::summary::load(request, expanded);
    }
    let dir = request.dir.as_path();
    let (base, from) = comparison(request, dir)?;
    let mut args = vec!["diff", from.as_str()];
    if request.mode == Mode::Commits {
        args.push("HEAD");
    }
    args.extend(DIFF);
    let patch = run(dir, &args)?;
    let untracked = if request.mode == Mode::Commits { Vec::new() } else { untracked(dir)? };
    let mut hasher = DefaultHasher::new();
    (&base, &patch, &untracked).hash(&mut hasher);
    let digest = hasher.finish();
    if request.last == Some(digest) {
        return Ok(Loaded { base, digest, diff: None });
    }
    let files = files_of(request, &from, &patch, &untracked);
    Ok(Loaded { base, digest, diff: Some(Diff { files }) })
}

fn comparison(request: &Request, dir: &Path) -> Result<(Option<String>, String)> {
    let base = match request.mode {
        Mode::Uncommitted => None,
        Mode::Commits | Mode::All => Some(
            request
                .base
                .clone()
                .or_else(|| default_base(dir))
                .ok_or_else(|| Error::Git("no default branch to compare with".into()))?,
        ),
    };
    let from = match (&base, request.mode) {
        (Some(base), Mode::Commits | Mode::All) => merge_base(dir, base)?,
        _ => head(dir)?,
    };
    Ok((base, from))
}

fn files_of(request: &Request, from: &str, patch: &[u8], untracked: &[Untracked]) -> Vec<Arc<File>> {
    let dir = request.dir.as_path();
    let previous = request.previous.iter().map(|f| (f.path.as_str(), f)).collect();
    let reviewed = |mut file: File| {
        if request.mode == Mode::Uncommitted {
            file.viewed_digest = fingerprint(dir, from, &file);
        }
        reuse(file, &previous)
    };
    let mut files: Vec<Arc<File>> = diff::parse(&String::from_utf8_lossy(patch)).into_iter().map(reviewed).collect();
    files.extend(untracked.iter().map(|u| reviewed(diff::untracked(&u.path, u.size, u.bytes.as_deref()))));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

pub(super) fn reuse(mut file: File, previous: &HashMap<&str, &Arc<File>>) -> Arc<File> {
    if let Some(old) = previous
        .get(file.path.as_str())
        .filter(|old| old.digest == file.digest && old.hunks.is_empty() == file.hunks.is_empty())
    {
        if old.viewed_digest == file.viewed_digest {
            return Arc::clone(old);
        }
        let mut reused = File::clone(old);
        reused.viewed_digest = file.viewed_digest;
        return Arc::new(reused);
    }
    diff::finish(&mut file);
    Arc::new(file)
}

pub(super) fn stamp(path: &Path) -> Option<(u64, i64, i64, i64, i64)> {
    std::fs::symlink_metadata(path).ok().map(|m| (m.len(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec()))
}

pub(super) fn fingerprint(dir: &Path, from: &str, file: &File) -> u64 {
    let stamp = stamp(&dir.join(&file.path));
    let mut hasher = DefaultHasher::new();
    (from, &file.path, &file.old_path, file.status, file.added, file.removed, stamp).hash(&mut hasher);
    hasher.finish()
}

pub(super) fn patch(dir: &Path, from: &str, paths: &[&str]) -> Result<Vec<u8>> {
    let mut args = vec!["diff", from];
    args.extend(DIFF);
    args.push("--");
    args.extend(paths);
    run(dir, &args)
}

pub(crate) fn new_side(dir: &Path, mode: Mode, path: &str) -> Option<Vec<String>> {
    let bytes = match mode {
        Mode::Commits => run(dir, &["show", &format!("HEAD:./{path}")]).ok()?,
        Mode::Uncommitted | Mode::All => read(&dir.join(path))?,
    };
    if bytes.len() as u64 > MAX_UNTRACKED_BYTES {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).lines().map(diff::expand).collect())
}

#[cfg(test)]
mod tests;
