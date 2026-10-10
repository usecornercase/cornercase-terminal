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
use crate::worktree;

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
pub struct Request {
    pub dir: PathBuf,
    pub mode: Mode,
    pub base: Option<String>,
    pub last: Option<u64>,
    pub previous: Vec<Arc<File>>,
    pub expanded: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct Loaded {
    pub base: Option<String>,
    pub digest: u64,
    pub diff: Option<Diff>,
}

pub(super) fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    worktree::check(worktree::git(dir, GLOBAL.iter().chain(args))?)
}

fn text(dir: &Path, args: &[&str]) -> Option<String> {
    let out = run(dir, args).ok()?;
    let text = String::from_utf8_lossy(&out).trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn default_base(dir: &Path) -> Option<String> {
    if let Some(head) = text(dir, &["symbolic-ref", "--quiet", REMOTE_HEAD]) {
        return Some(head.strip_prefix("refs/remotes/").unwrap_or(&head).to_string());
    }
    DEFAULT_BRANCHES
        .into_iter()
        .find(|name| text(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_some())
        .map(str::to_string)
}

pub fn branches(dir: &Path) -> Vec<String> {
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
    pub path: String,
    pub size: u64,
    pub bytes: Option<Vec<u8>>,
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

pub fn load(request: &Request) -> Result<Loaded> {
    if let Some(expanded) = &request.expanded {
        return super::summary::load(request, expanded);
    }
    let dir = request.dir.as_path();
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
    let previous = request.previous.iter().map(|f| (f.path.as_str(), f)).collect();
    let reviewed = |mut file: File| {
        if request.mode == Mode::Uncommitted {
            file.viewed_digest = fingerprint(dir, &from, &file);
        }
        reuse(file, &previous)
    };
    let mut files: Vec<Arc<File>> = diff::parse(&String::from_utf8_lossy(&patch)).into_iter().map(reviewed).collect();
    files.extend(untracked.iter().map(|u| reviewed(diff::untracked(&u.path, u.size, u.bytes.as_deref()))));
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded { base, digest, diff: Some(Diff { files }) })
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

pub fn new_side(dir: &Path, mode: Mode, path: &str) -> Option<Vec<String>> {
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
mod tests {
    use super::*;
    use crate::changes::diff::Status;
    use crate::test_util::{TempDir, git, git_repo};

    fn request(dir: &Path, mode: Mode) -> Request {
        Request { dir: dir.to_path_buf(), mode, base: None, last: None, previous: Vec::new(), expanded: None }
    }

    fn files(dir: &Path, mode: Mode) -> Vec<(String, Status, usize, usize)> {
        let loaded = load(&request(dir, mode)).expect("load");
        let diff = loaded.diff.expect("a diff");
        diff.files.iter().map(|f| (f.path.clone(), f.status, f.added, f.removed)).collect()
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        std::fs::write(dir.join(name), contents).expect("write file");
    }

    fn feature_branch() -> TempDir {
        let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
        git(repo.path(), &["switch", "--quiet", "-c", "feature"]);
        write(repo.path(), "a.txt", "one\nTWO\n");
        git(repo.path(), &["commit", "--quiet", "-am", "change a"]);
        write(repo.path(), "b.txt", "new\n");
        repo
    }

    #[test]
    fn uncommitted_shows_edits_and_new_files() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "one\nmore\n");
        write(repo.path(), "new.txt", "x\ny\n");

        assert_eq!(
            files(repo.path(), Mode::Uncommitted),
            [("a.txt".into(), Status::Modified, 1, 0), ("new.txt".into(), Status::Untracked, 2, 0)]
        );
    }

    #[test]
    fn staged_changes_count_as_uncommitted() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "two\n");
        git(repo.path(), &["add", "a.txt"]);

        assert_eq!(files(repo.path(), Mode::Uncommitted), [("a.txt".into(), Status::Modified, 1, 1)]);
    }

    #[test]
    fn commits_shows_only_what_the_branch_committed() {
        let repo = feature_branch();
        assert_eq!(files(repo.path(), Mode::Commits), [("a.txt".into(), Status::Modified, 1, 1)]);
    }

    #[test]
    fn all_adds_what_is_not_committed_yet() {
        let repo = feature_branch();
        assert_eq!(
            files(repo.path(), Mode::All),
            [("a.txt".into(), Status::Modified, 1, 1), ("b.txt".into(), Status::Untracked, 1, 0)]
        );
    }

    #[test]
    fn compares_from_where_the_branch_left_the_base() {
        let repo = feature_branch();
        git(repo.path(), &["switch", "--quiet", "main"]);
        write(repo.path(), "c.txt", "on main\n");
        git(repo.path(), &["add", "c.txt"]);
        git(repo.path(), &["commit", "--quiet", "-m", "main moves on"]);
        git(repo.path(), &["switch", "--quiet", "feature"]);

        assert_eq!(files(repo.path(), Mode::Commits), [("a.txt".into(), Status::Modified, 1, 1)]);
    }

    #[test]
    fn the_base_can_be_any_branch() {
        let repo = feature_branch();
        git(repo.path(), &["branch", "other", "HEAD"]);
        let loaded =
            load(&Request { base: Some("other".into()), ..request(repo.path(), Mode::Commits) }).expect("load");
        assert_eq!((loaded.base.as_deref(), loaded.diff.expect("diff").files.len()), (Some("other"), 0));
    }

    #[test]
    fn a_missing_base_is_an_error() {
        let repo = feature_branch();
        let result = load(&Request { base: Some("nope".into()), ..request(repo.path(), Mode::Commits) });
        assert_eq!(result.err().map(|e| e.to_string()), Some("cannot compare with nope".into()));
    }

    #[test]
    fn the_default_base_is_the_remote_head() {
        let remote = git_repo(&[("a.txt", "x\n")]);
        let tmp = TempDir::new();
        git(tmp.path(), &["clone", "--quiet", &remote.path().display().to_string(), "clone"]);
        assert_eq!(default_base(&tmp.path().join("clone")).as_deref(), Some("origin/main"));
    }

    #[test]
    fn the_default_base_falls_back_to_main() {
        let repo = git_repo(&[]);
        assert_eq!(default_base(repo.path()).as_deref(), Some("main"));
    }

    #[test]
    fn lists_local_and_remote_branches() {
        let remote = git_repo(&[]);
        let tmp = TempDir::new();
        git(tmp.path(), &["clone", "--quiet", &remote.path().display().to_string(), "clone"]);
        let clone = tmp.path().join("clone");
        git(&clone, &["branch", "topic"]);
        assert_eq!(branches(&clone), ["main", "topic", "origin/main"]);
    }

    #[test]
    fn an_unchanged_repo_reports_no_new_diff() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "two\n");
        let first = load(&request(repo.path(), Mode::Uncommitted)).expect("load");
        let again =
            load(&Request { last: Some(first.digest), ..request(repo.path(), Mode::Uncommitted) }).expect("load");
        assert!(again.diff.is_none());
    }

    #[test]
    fn unchanged_files_are_reused() {
        let repo = git_repo(&[("a.txt", "one\n"), ("b.txt", "one\n")]);
        write(repo.path(), "a.txt", "two\n");
        let first = load(&request(repo.path(), Mode::Uncommitted)).expect("load").diff.expect("diff");
        write(repo.path(), "b.txt", "two\n");
        let previous = first.files.clone();
        let again = load(&Request { previous, ..request(repo.path(), Mode::Uncommitted) }).expect("load");
        let again = again.diff.expect("diff");
        assert!(Arc::ptr_eq(&first.files[0], &again.files[0]));
    }

    #[rstest::rstest]
    #[case::project(false)]
    #[case::every_project(true)]
    fn an_equal_count_edit_between_the_patch_and_metadata_read_does_not_reuse_the_old_patch(#[case] every: bool) {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "two\n");
        let from = head(repo.path()).expect("head");
        let bytes = patch(repo.path(), &from, &["a.txt"]).expect("old patch");
        let mut old = diff::parse(&String::from_utf8_lossy(&bytes)).remove(0);
        write(repo.path(), "a.txt", "six\n");
        old.viewed_digest = fingerprint(repo.path(), &from, &old);
        diff::finish(&mut old);
        let old = Arc::new(old);
        let request = Request {
            previous: vec![Arc::clone(&old)],
            expanded: every.then(|| vec!["a.txt".into()]),
            ..request(repo.path(), Mode::Uncommitted)
        };
        let loaded = load(&request).expect("new patch").diff.expect("diff");
        let new = &loaded.files[0];
        assert_eq!(new.viewed_digest, old.viewed_digest);
        assert_ne!(new.digest, old.digest);
        assert!(!Arc::ptr_eq(&old, new));
        assert_eq!(new.hunks[0].lines[1].text, "six");
        let mut panel = crate::changes::Panel::default();
        panel.set_gap(1, &old, 1, Vec::new());
        assert!(panel.gap(1, new, 1).is_none());
    }

    #[test]
    fn viewed_marks_survive_a_scope_switch_without_reading_the_folded_patch() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "two\n");
        let request = request(repo.path(), Mode::Uncommitted);
        let project = load(&request).expect("project").diff.expect("diff");
        let every = load(&Request { expanded: Some(Vec::new()), ..request }).expect("summary").diff.expect("diff");
        assert_eq!(every.files[0].hunks, Vec::new());
        assert_ne!(every.files[0].digest, project.files[0].digest);
        let mut panel = crate::changes::Panel::default();
        panel.toggle_viewed(1, &project.files[0]);
        assert!(panel.viewed(1, &every.files[0]));
    }

    #[test]
    fn a_repo_without_commits_shows_its_files() {
        let repo = TempDir::new();
        git(repo.path(), &["init", "--quiet"]);
        write(repo.path(), "first.txt", "hi\n");
        assert_eq!(files(repo.path(), Mode::Uncommitted), [("first.txt".into(), Status::Untracked, 1, 0)]);
    }

    #[test]
    fn paths_are_relative_to_a_subfolder() {
        let repo = git_repo(&[("app/a.txt", "one\n"), ("other.txt", "one\n")]);
        write(&repo.path().join("app"), "a.txt", "two\n");
        write(repo.path(), "other.txt", "two\n");
        let paths: Vec<String> = files(&repo.path().join("app"), Mode::Uncommitted).into_iter().map(|f| f.0).collect();
        assert_eq!(paths, ["a.txt"]);
    }

    #[test]
    fn does_not_rewrite_the_index() {
        let repo = git_repo(&[("a.txt", "one\n"), ("b.txt", "one\n")]);
        let index = repo.path().join(".git/index");
        let before = std::fs::metadata(&index).and_then(|m| m.modified()).expect("index time");
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        for name in ["a.txt", "b.txt"] {
            let file = std::fs::File::options().write(true).open(repo.path().join(name)).expect("open");
            file.set_modified(later).expect("touch");
        }
        files(repo.path(), Mode::Uncommitted);
        let after = std::fs::metadata(&index).and_then(|m| m.modified()).expect("index time");
        assert_eq!(before, after);
    }

    #[test]
    fn an_untracked_symlink_shows_its_target_not_its_contents() {
        let repo = git_repo(&[]);
        let outside = TempDir::new();
        std::fs::write(outside.path().join("secret.txt"), "secret\n").expect("write target");
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), repo.path().join("link")).expect("symlink");
        let diff = load(&request(repo.path(), Mode::Uncommitted)).expect("load").diff.expect("diff");
        let line = &diff.files[0].hunks[0].lines[0].text;
        assert_eq!(line, &outside.path().join("secret.txt").display().to_string());
    }

    #[test]
    fn reads_the_new_side_of_a_file() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        write(repo.path(), "a.txt", "one\n\ttwo\n");
        assert_eq!(new_side(repo.path(), Mode::Uncommitted, "a.txt"), Some(vec!["one".into(), "    two".into()]));
        assert_eq!(new_side(repo.path(), Mode::Commits, "a.txt"), Some(vec!["one".into()]));
    }
}
