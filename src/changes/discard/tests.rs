use rstest::rstest;

use super::*;
use crate::changes::tests::{digest, new_change, status, tracked_change, write};
use crate::test_util::{TempDir, git, git_repo};

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
        assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
        assert_eq!(status(dir, &["a.txt", "b.txt", "c.txt", "new"]), "");
        assert_eq!(status(dir, &["other.txt"]), " M other.txt\n");
        assert_eq!(std::fs::read_to_string(dir.join("other.txt")).expect("other"), "changed\n");
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
        assert!(matches!(outcome, Outcome::Done(_)));
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
        assert!(matches!(outcome, Outcome::Done(_)));
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
