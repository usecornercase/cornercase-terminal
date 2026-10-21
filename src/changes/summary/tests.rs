use std::time::{Duration, Instant};

use super::*;
use crate::changes::{Checkout, Mode, Panel, Scope};
use crate::test_util::{TempDir, git, git_repo};

fn request(dir: &Path, paths: &[&str]) -> Request {
    Request {
        dir: dir.to_path_buf(),
        mode: Mode::Uncommitted,
        base: None,
        last: None,
        previous: Vec::new(),
        expanded: Some(paths.iter().map(|s| (*s).to_string()).collect()),
    }
}

#[rstest::rstest]
#[case::modified("edit")]
#[case::staged("stage")]
#[case::renamed("rename")]
#[case::deleted("delete")]
#[case::binary("binary")]
#[case::untracked("untracked")]
#[case::unborn("unborn")]
#[case::lockfile("lockfile")]
#[case::large_tracked("large")]
#[case::large_lockfile("large-lockfile")]
#[case::large_replacement("large-replacement")]
#[case::large_untracked("large-untracked")]
#[case::at_the_limit("limit")]
fn folded_counts_match_the_project_diff(#[case] change: &str) {
    let path = if change.ends_with("lockfile") { "Cargo.lock" } else { "a.txt" };
    let original =
        if change == "large-replacement" { "old\n".repeat(diff::MAX_LINES / 2 + 1) } else { "one\ntwo\n".to_string() };
    let repo = if change == "unborn" {
        let repo = TempDir::new();
        git(repo.path(), &["init", "--quiet"]);
        repo
    } else {
        git_repo(&[(path, &original)])
    };
    match change {
        "rename" => git(repo.path(), &["mv", "a.txt", "a new.txt"]),
        "delete" => std::fs::remove_file(repo.path().join("a.txt")).expect("delete"),
        "binary" => std::fs::write(repo.path().join("a.txt"), b"a\0b").expect("binary"),
        "untracked" | "unborn" => std::fs::write(repo.path().join("new.txt"), "new\nlines\n").expect("new"),
        "large" | "large-lockfile" | "large-replacement" | "large-untracked" | "limit" => {
            let lines = match change {
                "large-replacement" => diff::MAX_LINES / 2 + 1,
                "limit" => diff::MAX_LINES - 2,
                _ => diff::MAX_LINES + 1,
            };
            let path = if change == "large-untracked" { "new.txt" } else { path };
            std::fs::write(repo.path().join(path), "new\n".repeat(lines)).expect("many changed lines");
        }
        _ => {
            std::fs::write(repo.path().join(path), "one\nTWO\n").expect("edit");
            if change == "stage" {
                git(repo.path(), &["add", "a.txt"]);
            }
        }
    }
    let summary = git::load(&request(repo.path(), &[])).expect("summary").diff.expect("diff");
    let full = git::load(&Request { expanded: None, ..request(repo.path(), &[]) }).expect("full").diff.expect("diff");
    let counts = |diff: &Diff| {
        diff.files
            .iter()
            .map(|f| {
                (f.path.clone(), f.old_path.clone(), f.status, f.added, f.removed, f.fold, f.viewed_digest, f.staged)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(counts(&summary), counts(&full));
    assert!(summary.files.iter().all(|f| f.hunks.is_empty()));
}

#[rstest::rstest]
#[case::generated_file("generated.txt")]
#[case::lockfile("Cargo.lock")]
fn an_unfold_request_keeps_large_tracked_files_folded_without_reading_their_patch(#[case] path: &str) {
    let repo = git_repo(&[(path, "one\n")]);
    std::fs::write(repo.path().join(path), "new\n".repeat(diff::MAX_LINES + 1)).expect("large edit");
    let targets = [(Checkout { workspace: 1, dir: repo.path().to_path_buf(), base: None }, false)];
    let now = Instant::now();
    let mut panel = Panel { scope: Scope::Every, watched: true, ..Panel::default() };
    let (workspace, generation, request) = panel.request_every(&targets, now).expect("summary request");
    let result = git::load(&request).expect("summary");
    panel.loaded(workspace, generation, &request, Ok(result));
    let initial = panel.summary(workspace).expect("model").diff().expect("diff").clone();
    panel.set_fold(workspace, &initial.files[0], false);
    for visit in 0..3 {
        let (_, generation, request) = panel
            .request_every(&targets, now + Duration::from_secs(visit * 3))
            .expect("refresh after the unfold request");
        assert_eq!(request.expanded, Some(Vec::new()));
        let result = git::load(&request).expect("summary refresh");
        assert!(result.diff.is_none());
        panel.loaded(workspace, generation, &request, Ok(result));
        let diff = panel.summary(workspace).expect("model").diff().expect("diff");
        let file = &diff.files[0];
        assert_eq!((file.fold, file.added, file.removed, file.hunks.len()), (Fold::Large, diff::MAX_LINES + 1, 1, 0));
        assert!(panel.folded(workspace, diff, file));
    }
}

#[test]
fn reads_only_open_files_and_refreshes_equal_count_edits() {
    let repo = git_repo(&[("a.txt", "one\n"), ("b.txt", "one\n")]);
    for path in ["a.txt", "b.txt"] {
        std::fs::write(repo.path().join(path), "two\n").expect("edit");
    }
    let mut request = request(repo.path(), &[]);
    let first = git::load(&request).expect("folded");
    request.last = Some(first.digest);
    request.previous = first.diff.expect("diff").files;
    assert!(git::load(&request).expect("unchanged").diff.is_none());
    request.expanded = Some(vec!["a.txt".into()]);
    let opened = git::load(&request).expect("open");
    let files = opened.diff.expect("diff").files;
    assert_eq!(files[0].hunks.len(), 1);
    assert_eq!(files[1].hunks, Vec::new());
    request.last = Some(opened.digest);
    request.previous = files;
    std::fs::write(repo.path().join("a.txt"), "six\n").expect("same counts");
    let changed = git::load(&request).expect("refresh").diff.expect("changed");
    assert_ne!(changed.files[0].digest, request.previous[0].digest);
    assert_eq!(changed.files[0].hunks[0].lines[1].text, "six");
}

#[test]
fn unfolding_a_rename_preserves_its_old_path_status_and_line_counts() {
    let repo = git_repo(&[("old.txt", "one\ntwo\nthree\nfour\n")]);
    git(repo.path(), &["mv", "old.txt", "new.txt"]);
    std::fs::write(repo.path().join("new.txt"), "one\nTWO\nthree\nfour\n").expect("edit");
    let mut request = request(repo.path(), &[]);
    let folded = git::load(&request).expect("folded").diff.expect("diff");
    request.previous = folded.files.clone();
    request.expanded = Some(vec!["new.txt".into()]);
    let open = git::load(&request).expect("open").diff.expect("diff");
    let file = &open.files[0];
    assert_eq!(
        (file.status, file.old_path.as_deref(), file.added, file.removed),
        (Status::Renamed, Some("old.txt"), 1, 1)
    );
    assert_eq!(file.viewed_digest, folded.files[0].viewed_digest);
    assert_eq!(file.hunks.len(), 1);
    assert!(file.hunks[0].lines.iter().any(|l| l.text == "TWO"));
    request.expanded = Some(Vec::new());
    request.previous = open.files;
    let again = git::load(&request).expect("fold again").diff.expect("diff");
    assert_eq!(again.files, folded.files);
}

#[rstest::rstest]
#[case::main_worktree(false)]
#[case::linked_worktree(true)]
fn a_subfolder_excludes_changes_outside_it_without_resolving_its_prefix_with_git(#[case] linked: bool) {
    let repo = git_repo(&[("app/a.txt", "one\n"), ("b.txt", "one\n")]);
    let checkout = TempDir::new();
    let path = checkout.path().join("topic");
    let root = if linked {
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "topic", path.to_str().expect("path")]);
        path.as_path()
    } else {
        repo.path()
    };
    std::fs::write(root.join("app/a.txt"), "two\n").expect("inside");
    std::fs::write(root.join("b.txt"), "two\n").expect("outside");
    std::fs::write(root.join("app/new.txt"), "new\n").expect("new");
    let loaded = git::load(&request(&root.join("app"), &[])).expect("summary").diff.expect("diff");
    assert_eq!(
        loaded.files.iter().map(|f| (f.path.as_str(), f.status)).collect::<Vec<_>>(),
        [("a.txt", Status::Modified), ("new.txt", Status::Untracked)]
    );
}

#[test]
fn a_summary_leaves_the_index_and_its_lock_alone() {
    let repo = git_repo(&[("a.txt", "one\n")]);
    std::fs::write(repo.path().join("a.txt"), "two\n").expect("edit");
    let index = repo.path().join(".git/index");
    let before = std::fs::read(&index).expect("index");
    let time = std::fs::metadata(&index).expect("metadata").modified().expect("time");
    std::fs::write(repo.path().join(".git/index.lock"), "held").expect("lock");
    git::load(&request(repo.path(), &["a.txt"])).expect("locked index");
    assert_eq!(std::fs::read(index).expect("index"), before);
    assert_eq!(std::fs::metadata(repo.path().join(".git/index")).expect("metadata").modified().expect("time"), time);
}
