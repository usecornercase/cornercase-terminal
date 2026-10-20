use std::path::Path;

use rstest::rstest;

use super::*;
use crate::changes::Mode;
use crate::test_util::{TempDir, git, git_repo};

fn digest(dir: &Path, path: &str) -> u64 {
    let request = git::Request {
        dir: dir.to_path_buf(),
        mode: Mode::Uncommitted,
        base: None,
        last: None,
        previous: Vec::new(),
        expanded: None,
    };
    let diff = git::load(&request).expect("load").diff.expect("diff");
    diff.files.iter().find(|f| f.path == path).expect("the file is listed").viewed_digest
}

fn status(dir: &Path, paths: &[&str]) -> String {
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(paths);
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(dir: &Path, path: &str, text: &str) {
    std::fs::write(dir.join(path), text).expect("write file");
}

fn tracked_change(dir: &Path, kind: &str) {
    match kind {
        "modified" => write(dir, "a.txt", "one\nTWO\n"),
        "staged_and_unstaged" => {
            write(dir, "a.txt", "one\nTWO\n");
            git(dir, &["add", "a.txt"]);
            write(dir, "a.txt", "one\nTWO\nthree\n");
        }
        "deleted" => std::fs::remove_file(dir.join("a.txt")).expect("remove"),
        "deleted_with_git_rm" => git(dir, &["rm", "--quiet", "a.txt"]),
        "renamed" => git(dir, &["mv", "a.txt", "c.txt"]),
        _ => {
            git(dir, &["mv", "a.txt", "c.txt"]);
            write(dir, "c.txt", "one\ntwo\nthree\n");
        }
    }
}

fn new_change(dir: &Path, kind: &str) {
    match kind {
        "added_in_a_new_folder" => {
            std::fs::create_dir(dir.join("new")).expect("folder");
            write(dir, "new/b.txt", "b\n");
            git(dir, &["add", "new/b.txt"]);
        }
        "intent_to_add" => {
            write(dir, "b.txt", "b\n");
            git(dir, &["add", "-N", "b.txt"]);
        }
        _ => write(dir, "b.txt", "b\n"),
    }
}

mod puts_the_path_back {
    use super::*;

    #[rstest]
    #[case::modified("modified", "a.txt")]
    #[case::staged_and_unstaged("staged_and_unstaged", "a.txt")]
    #[case::deleted("deleted", "a.txt")]
    #[case::deleted_with_git_rm("deleted_with_git_rm", "a.txt")]
    #[case::added_in_a_new_folder("added_in_a_new_folder", "new/b.txt")]
    #[case::intent_to_add("intent_to_add", "b.txt")]
    #[case::untracked("untracked", "b.txt")]
    #[case::renamed("renamed", "c.txt")]
    #[case::renamed_and_edited("renamed_and_edited", "c.txt")]
    fn and_touches_no_other_file(#[case] kind: &str, #[case] path: &str) {
        let repo = git_repo(&[("a.txt", "one\ntwo\n"), ("other.txt", "x\n")]);
        let dir = repo.path();
        write(dir, "other.txt", "changed\n");
        if path == "b.txt" || path == "new/b.txt" {
            new_change(dir, kind);
        } else {
            tracked_change(dir, kind);
        }
        let outcome = discard(dir, path, digest(dir, path)).expect("discard");
        assert!(matches!(outcome, Outcome::Discarded(_)), "{outcome:?}");
        assert_eq!(status(dir, &["a.txt", "b.txt", "c.txt", "new"]), "");
        assert_eq!(status(dir, &["other.txt"]), " M other.txt\n");
        assert_eq!(std::fs::read_to_string(dir.join("other.txt")).expect("other"), "changed\n");
    }

    #[test]
    fn a_name_with_glob_characters_touches_no_other_file() {
        let repo = git_repo(&[("star*.txt", "one\n"), ("starA.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "star*.txt", "two\n");
        write(dir, "starA.txt", "two\n");
        let outcome = discard(dir, "star*.txt", digest(dir, "star*.txt")).expect("discard");
        assert!(matches!(outcome, Outcome::Discarded(_)));
        assert_eq!(status(dir, &[":(literal)star*.txt"]), "");
        assert_eq!(std::fs::read_to_string(dir.join("starA.txt")).expect("read"), "two\n");
    }

    #[test]
    fn a_workspace_in_a_subfolder_discards_its_own_path() {
        let repo = git_repo(&[("proj/p.txt", "one\n"), ("top.txt", "one\n")]);
        let (dir, sub) = (repo.path(), repo.path().join("proj"));
        write(dir, "proj/p.txt", "two\n");
        write(dir, "top.txt", "two\n");
        let outcome = discard(&sub, "p.txt", digest(&sub, "p.txt")).expect("discard");
        assert!(matches!(outcome, Outcome::Discarded(_)));
        assert_eq!(status(dir, &["proj"]), "");
        assert_eq!(std::fs::read_to_string(dir.join("top.txt")).expect("read"), "two\n");
    }
}

mod refuses {
    use super::*;

    #[test]
    fn a_file_changed_after_the_digest() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "a.txt", "two\n");
        let seen = digest(dir, "a.txt");
        write(dir, "a.txt", "three, longer\n");
        assert!(matches!(discard(dir, "a.txt", seen).expect("discard"), Outcome::Changed));
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).expect("read"), "three, longer\n");
    }

    #[test]
    fn a_path_git_does_not_list() {
        let outer = TempDir::new();
        let dir = outer.path().join("repo");
        std::fs::create_dir(&dir).expect("folder");
        git(&dir, &["init", "--quiet"]);
        write(&dir, "a.txt", "one\n");
        git(&dir, &["add", "--all"]);
        git(&dir, &["commit", "--quiet", "-m", "init"]);
        write(outer.path(), "outside.txt", "keep\n");
        for path in ["../outside.txt", "nope.txt"] {
            assert!(matches!(discard(&dir, path, 0).expect("discard"), Outcome::Gone), "{path}");
        }
        assert_eq!(std::fs::read_to_string(outer.path().join("outside.txt")).expect("read"), "keep\n");
    }

    #[test]
    fn a_held_index_lock_with_gits_message() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "a.txt", "two\n");
        let seen = digest(dir, "a.txt");
        write(dir, ".git/index.lock", "");
        let error = discard(dir, "a.txt", seen).expect_err("locked");
        assert!(error.to_string().contains("index.lock"), "{error}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).expect("read"), "two\n");
    }
}

mod deletes {
    use super::*;

    #[test]
    fn a_symlink_removes_the_link_only() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let elsewhere = TempDir::new();
        write(elsewhere.path(), "target.txt", "keep\n");
        let link = repo.path().join("link");
        std::os::unix::fs::symlink(elsewhere.path().join("target.txt"), &link).expect("symlink");
        let outcome = discard(repo.path(), "link", digest(repo.path(), "link")).expect("discard");
        assert!(matches!(outcome, Outcome::Discarded(_)));
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert_eq!(std::fs::read_to_string(elsewhere.path().join("target.txt")).expect("read"), "keep\n");
    }

    #[test]
    fn an_added_file_on_an_unborn_branch() {
        let repo = TempDir::new();
        let dir = repo.path();
        git(dir, &["init", "--quiet"]);
        write(dir, "a.txt", "one\n");
        git(dir, &["add", "a.txt"]);
        let outcome = discard(dir, "a.txt", digest(dir, "a.txt")).expect("discard");
        assert!(matches!(outcome, Outcome::Discarded(_)));
        assert!(!dir.join("a.txt").exists());
        assert_eq!(status(dir, &[]), "");
    }
}

mod says {
    use super::*;

    #[rstest]
    #[case::modified(Status::Modified, None, "Discard the changes to a.txt? This cannot be undone.")]
    #[case::deleted(Status::Deleted, None, "Discard the changes to a.txt? This cannot be undone.")]
    #[case::added(
        Status::Added,
        None,
        "Discard the changes to a.txt? It is new, so it is deleted. This cannot be undone."
    )]
    #[case::untracked(Status::Untracked, None, "Delete a.txt? It is not in git, so this cannot be undone.")]
    #[case::renamed(
        Status::Renamed,
        Some("old.txt"),
        "Discard the changes to old.txt → a.txt? old.txt comes back as it was in the last commit and a.txt is \
         deleted. This cannot be undone."
    )]
    fn what_goes(#[case] status: Status, #[case] old: Option<&str>, #[case] text: &str) {
        assert_eq!(question("a.txt", old, status), text);
    }

    #[rstest]
    #[case::modified(Status::Modified, None, "discarded the changes to a.txt")]
    #[case::deleted(Status::Deleted, None, "discarded the changes to a.txt")]
    #[case::added(Status::Added, None, "deleted a.txt")]
    #[case::untracked(Status::Untracked, None, "deleted a.txt")]
    #[case::renamed(Status::Renamed, Some("old.txt"), "discarded the changes to old.txt → a.txt")]
    fn what_went(#[case] status: Status, #[case] old: Option<&str>, #[case] text: &str) {
        assert_eq!(done("a.txt", old, status), text);
    }
}
