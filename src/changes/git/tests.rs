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
    let loaded = load(&Request { base: Some("other".into()), ..request(repo.path(), Mode::Commits) }).expect("load");
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
    let (_tmp, clone) = crate::test_util::clone_of(remote.path());
    assert_eq!(default_base(&clone).as_deref(), Some("origin/main"));
}

#[test]
fn the_default_base_falls_back_to_main() {
    let repo = git_repo(&[]);
    assert_eq!(default_base(repo.path()).as_deref(), Some("main"));
}

#[test]
fn lists_local_and_remote_branches() {
    let remote = git_repo(&[]);
    let (_tmp, clone) = crate::test_util::clone_of(remote.path());
    git(&clone, &["branch", "topic"]);
    assert_eq!(branches(&clone), ["main", "topic", "origin/main"]);
}

#[test]
fn an_unchanged_repo_reports_no_new_diff() {
    let repo = git_repo(&[("a.txt", "one\n")]);
    write(repo.path(), "a.txt", "two\n");
    let first = load(&request(repo.path(), Mode::Uncommitted)).expect("load");
    let again = load(&Request { last: Some(first.digest), ..request(repo.path(), Mode::Uncommitted) }).expect("load");
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
