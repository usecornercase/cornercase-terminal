use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

use super::diff::MAX_UNTRACKED_BYTES;
use super::git::{self, Deadline, UNTRACKED_READ_BYTES};
use crate::error::{Error, Result};
use crate::git::worktree;
use crate::os::ffi;
use crate::os::fs::DirBuilderExt;

const IDENTITY: &str = "cornercase";
const IDENTITY_EMAIL: &str = "cornercase@localhost";
const TOO_OLD: &str = "the merge check needs git 2.38 or later";
const USAGE_EXIT: i32 = 129;

static SCRATCHES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Default)]
pub(crate) struct Check {
    pub(crate) dir: PathBuf,
    pub(crate) base: Option<String>,
    pub(crate) with_base: bool,
    pub(crate) others: Vec<(u64, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum With {
    Base(Option<String>),
    Workspace(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conflict {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Merged {
    pub(crate) conflicts: Vec<Conflict>,
    pub(crate) left_out: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Merge {
    pub(crate) with: With,
    pub(crate) result: std::result::Result<Merged, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Checked {
    pub(crate) merges: Vec<Merge>,
}

pub(crate) fn check(check: &Check) -> Checked {
    let _deadline = Deadline::arm();
    let mut run = match Scratch::new() {
        Ok(scratch) => Run { scratch, taken: HashMap::new() },
        Err(e) => return failed(check, &e.to_string()),
    };
    let mut merges = Vec::new();
    if check.with_base {
        merges.push(run.merge_base(check));
    }
    for (id, dir) in &check.others {
        merges.push(Merge { with: With::Workspace(*id), result: run.with_other(&check.dir, dir) });
    }
    Checked { merges }
}

pub(crate) fn failed(check: &Check, why: &str) -> Checked {
    let base = check.with_base.then(|| With::Base(check.base.clone()));
    let others = check.others.iter().map(|(id, _)| With::Workspace(*id));
    Checked { merges: base.into_iter().chain(others).map(|with| Merge { with, result: Err(why.into()) }).collect() }
}

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new() -> std::io::Result<Self> {
        Self::under(&std::env::temp_dir(), &SCRATCHES)
    }

    fn under(root: &Path, counter: &AtomicU64) -> std::io::Result<Self> {
        loop {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            let dir = root.join(format!("cornercase-merge-{}-{n}", std::process::id()));
            match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => {
                    let scratch = Self { dir };
                    std::fs::DirBuilder::new().mode(0o700).create(scratch.dir.join("objects"))?;
                    return Ok(scratch);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn quoted(path: &Path) -> OsString {
    let bytes = ffi::to_bytes(path);
    if !bytes.iter().any(|b| *b == ffi::LIST_SEPARATOR || *b == b'"') {
        return ffi::to_os(bytes.into_owned());
    }
    let mut out = vec![b'"'];
    for b in bytes.iter() {
        if matches!(b, b'\\' | b'"') {
            out.push(b'\\');
        }
        out.push(*b);
    }
    out.push(b'"');
    ffi::to_os(out)
}

struct Env(Vec<(&'static str, OsString)>);

impl Env {
    fn of(scratch: &Scratch, dir: &Path) -> Result<Self> {
        let objects = crate::git::objects_dir(dir).ok_or_else(|| Error::Git("not a git repository".into()))?;
        Ok(Self(vec![
            ("GIT_OBJECT_DIRECTORY", ffi::plain(&scratch.dir.join("objects")).into_owned().into_os_string()),
            ("GIT_ALTERNATE_OBJECT_DIRECTORIES", quoted(&objects)),
        ]))
    }

    fn snapshotting(mut self, index: &Path) -> Self {
        self.0.push(("GIT_INDEX_FILE", ffi::plain(index).into_owned().into_os_string()));
        for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] {
            self.0.push((key, IDENTITY.into()));
        }
        for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
            self.0.push((key, IDENTITY_EMAIL.into()));
        }
        self
    }

    fn pairs(&self) -> Vec<(&str, &OsStr)> {
        self.0.iter().map(|(key, value)| (*key, value.as_os_str())).collect()
    }

    fn raw(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        git::run_raw(dir, args, &self.pairs())
    }

    fn run(&self, dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
        worktree::check(self.raw(dir, args)?)
    }

    fn text(&self, dir: &Path, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(dir, args)?).trim().to_string())
    }
}

#[derive(Clone)]
struct Snapshot {
    commit: String,
    left_out: usize,
}

struct Run {
    scratch: Scratch,
    taken: HashMap<PathBuf, std::result::Result<Snapshot, String>>,
}

impl Run {
    fn merge_base(&mut self, check: &Check) -> Merge {
        let base = check.base.clone().or_else(|| git::default_base(&check.dir));
        let result = match &base {
            Some(base) => self.merge_into(&check.dir, base),
            None => Err("no base branch to merge into".into()),
        };
        Merge { with: With::Base(base), result }
    }

    fn merge_into(&mut self, dir: &Path, base: &str) -> std::result::Result<Merged, String> {
        let env = Env::of(&self.scratch, dir).map_err(|e| e.to_string())?;
        let spec = format!("{base}^{{commit}}");
        let found = env.text(dir, &["rev-parse", "--verify", "--quiet", &spec]).ok().filter(|sha| !sha.is_empty());
        let into = found.ok_or_else(|| format!("no branch {base}"))?;
        let work = self.snapshot(dir)?;
        let conflicts = merge(&env, dir, &into, &work.commit).map_err(|e| e.to_string())?;
        Ok(Merged { conflicts, left_out: work.left_out })
    }

    fn with_other(&mut self, dir: &Path, other: &Path) -> std::result::Result<Merged, String> {
        let ours = self.snapshot(dir)?;
        let theirs = self.snapshot(other)?;
        let env = Env::of(&self.scratch, dir).map_err(|e| e.to_string())?;
        let conflicts = merge(&env, dir, &ours.commit, &theirs.commit).map_err(|e| e.to_string())?;
        Ok(Merged { conflicts, left_out: ours.left_out + theirs.left_out })
    }

    fn snapshot(&mut self, dir: &Path) -> std::result::Result<Snapshot, String> {
        if let Some(taken) = self.taken.get(dir) {
            return taken.clone();
        }
        let index = self.scratch.dir.join(format!("index-{}", self.taken.len()));
        let taken = snapshot(&self.scratch, dir, &index).map_err(|e| e.to_string());
        self.taken.insert(dir.to_path_buf(), taken.clone());
        taken
    }
}

fn merge(env: &Env, dir: &Path, ours: &str, theirs: &str) -> Result<Vec<Conflict>> {
    let out = env.raw(dir, &["merge-tree", "--write-tree", "--name-only", "-z", "--messages", ours, theirs])?;
    match out.status.code() {
        Some(0) => Ok(Vec::new()),
        Some(1) => Ok(parse(&out.stdout)),
        _ => Err(failure(out)),
    }
}

fn failure(out: Output) -> Error {
    if out.status.code() == Some(USAGE_EXIT) || String::from_utf8_lossy(&out.stderr).contains("write-tree") {
        return Error::Git(TOO_OLD.into());
    }
    worktree::check(out).err().unwrap_or_else(|| Error::Git(TOO_OLD.into()))
}

fn snapshot(scratch: &Scratch, dir: &Path, index: &Path) -> Result<Snapshot> {
    let plain = Env::of(scratch, dir)?;
    let top = ffi::to_path(plain.text(dir, &["rev-parse", "--show-toplevel"])?.as_bytes());
    let head = plain.text(&top, &["rev-parse", "--verify", "--quiet", "HEAD"]).unwrap_or_default();
    if head.is_empty() {
        return Err(Error::Git("the branch has no commit yet".into()));
    }
    let env = Env::of(scratch, dir)?.snapshotting(index);
    match crate::git::index_file(dir).filter(|real| real.is_file()) {
        Some(real) => std::fs::copy(real, index).map(drop)?,
        None => drop(env.run(&top, &["read-tree", "HEAD"])?),
    }
    env.run(&top, &["-c", "core.splitIndex=false", "add", "-u"])?;
    let left_out = add_untracked(&env, scratch, &top, index)?;
    let tree = env.text(&top, &["write-tree"])?;
    let message = ["commit-tree", "--no-gpg-sign", "-p", &head, "-m", "cornercase merge check", &tree];
    Ok(Snapshot { commit: env.text(&top, &message)?, left_out })
}

fn add_untracked(env: &Env, scratch: &Scratch, top: &Path, index: &Path) -> Result<usize> {
    let listed = env.run(top, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let (kept, left_out) = within_caps(top, &listed);
    if kept.is_empty() {
        return Ok(left_out);
    }
    let file = scratch.dir.join(format!("untracked-{}", index.file_name().unwrap_or_default().to_string_lossy()));
    std::fs::write(&file, kept)?;
    let from = format!("--pathspec-from-file={}", ffi::text(&file));
    let args = ["--literal-pathspecs", "-c", "core.splitIndex=false", "add", &from, "--pathspec-file-nul"];
    env.run(top, &args)?;
    Ok(left_out)
}

fn within_caps(top: &Path, listed: &[u8]) -> (Vec<u8>, usize) {
    let mut kept = Vec::new();
    let mut left_out = 0;
    let mut budget = UNTRACKED_READ_BYTES;
    for path in listed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let Ok(meta) = std::fs::symlink_metadata(top.join(ffi::to_path(path))) else { continue };
        if !meta.is_symlink() && !meta.is_file() {
            continue;
        }
        if meta.is_file() && (meta.len() > MAX_UNTRACKED_BYTES || meta.len() > budget) {
            left_out += 1;
            continue;
        }
        budget = budget.saturating_sub(if meta.is_file() { meta.len() } else { 0 });
        kept.extend_from_slice(path);
        kept.push(0);
    }
    (kept, left_out)
}

pub(crate) fn parse(out: &[u8]) -> Vec<Conflict> {
    let fields: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    let paths: Vec<String> =
        fields.iter().skip(1).take_while(|f| !f.is_empty()).map(|f| String::from_utf8_lossy(f).into_owned()).collect();
    let records = records(fields.get(paths.len() + 2..).unwrap_or_default());
    paths
        .into_iter()
        .map(|path| {
            let found = records.iter().find(|(first, kind, _)| *first == path && kind.starts_with("CONFLICT"));
            let kind = found.and_then(|(_, kind, message)| {
                if message.starts_with("CONFLICT (add/add)") {
                    return Some("add/add");
                }
                kind.split_once('(')?.1.strip_suffix(')')
            });
            Conflict {
                kind: kind.unwrap_or("conflict").to_string(),
                message: found.map(|(_, _, message)| message.clone()).unwrap_or_default(),
                path,
            }
        })
        .collect()
}

fn records(mut fields: &[&[u8]]) -> Vec<(String, String, String)> {
    let mut records = Vec::new();
    while let Some(count) = fields.first().and_then(|f| String::from_utf8_lossy(f).parse::<usize>().ok()) {
        let Some(record) = fields.get(1..count + 3) else { break };
        let text = |i: usize| String::from_utf8_lossy(record[i]).into_owned();
        let first = if count > 0 { text(0) } else { String::new() };
        records.push((first, text(count), text(count + 1).trim().to_string()));
        fields = &fields[count + 3..];
    }
    records
}

#[cfg(test)]
mod tests;
