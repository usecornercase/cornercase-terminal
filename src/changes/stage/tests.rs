use rstest::rstest;

use super::*;
use crate::changes::act::Outcome;
use crate::changes::tests::{digest, new_change, status, tracked_change, write};
use crate::test_util::{TempDir, git, git_repo};

const PATHS: [&str; 4] = ["a.txt", "b.txt", "c.txt", "new"];

fn unstaged_side(text: &str) -> bool {
    text.lines().all(|line| line.as_bytes().get(1) == Some(&b' '))
}

fn nothing_staged(text: &str) -> bool {
    text.lines().all(|line| matches!(line.as_bytes().first(), Some(b' ' | b'?')))
}

mod stages_and_unstages {
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
    fn only_that_path(#[case] kind: &str, #[case] path: &str) {
        let repo = git_repo(&[("a.txt", "one\ntwo\n"), ("other.txt", "x\n")]);
        let dir = repo.path();
        write(dir, "other.txt", "changed\n");
        if path == "b.txt" || path == "new/b.txt" {
            new_change(dir, kind);
        } else {
            tracked_change(dir, kind);
        }
        let before: Vec<_> = PATHS.iter().filter_map(|p| std::fs::read_to_string(dir.join(p)).ok()).collect();
        let outcome = stage(dir, path, digest(dir, path), true).expect("stage");
        assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
        assert!(unstaged_side(&status(dir, &PATHS)), "{}", status(dir, &PATHS));
        assert_eq!(status(dir, &["other.txt"]), " M other.txt\n");
        let outcome = stage(dir, path, digest(dir, path), false).expect("unstage");
        assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
        assert!(nothing_staged(&status(dir, &PATHS)), "{}", status(dir, &PATHS));
        assert_eq!(status(dir, &["other.txt"]), " M other.txt\n");
        let after: Vec<_> = PATHS.iter().filter_map(|p| std::fs::read_to_string(dir.join(p)).ok()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn a_file_on_an_unborn_branch() {
        let repo = TempDir::new();
        let dir = repo.path();
        git(dir, &["init", "--quiet"]);
        write(dir, "a.txt", "one\n");
        stage(dir, "a.txt", digest(dir, "a.txt"), true).expect("stage");
        assert_eq!(status(dir, &[]), "A  a.txt\n");
        stage(dir, "a.txt", digest(dir, "a.txt"), false).expect("unstage");
        assert_eq!(status(dir, &[]), "?? a.txt\n");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).expect("read"), "one\n");
    }
}

mod keeps_the_review_identity {
    use super::*;

    #[test]
    fn of_a_modified_file() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "a.txt", "two\n");
        let seen = digest(dir, "a.txt");
        stage(dir, "a.txt", seen, true).expect("stage");
        assert_eq!(digest(dir, "a.txt"), seen);
    }

    #[test]
    fn of_an_untracked_file() {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "b.txt", "new\n");
        let seen = digest(dir, "b.txt");
        stage(dir, "b.txt", seen, true).expect("stage");
        assert_eq!(digest(dir, "b.txt"), seen);
    }
}

mod keeps_the_review_identity_of_untracked_files_whose_counts_change {
    use super::*;

    #[rstest]
    #[case::binary(b"a\0b\n".to_vec())]
    #[case::oversized(b"x\n".repeat(usize::try_from(crate::changes::diff::MAX_UNTRACKED_BYTES / 2).expect("fits") + 1))]
    fn when_staged(#[case] bytes: Vec<u8>) {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        std::fs::write(dir.join("b.bin"), bytes).expect("write");
        let seen = digest(dir, "b.bin");
        stage(dir, "b.bin", seen, true).expect("stage");
        assert_eq!(digest(dir, "b.bin"), seen);
    }
}

mod refuses {
    use super::*;

    #[rstest]
    #[case::stage(true)]
    #[case::unstage(false)]
    fn a_file_with_a_merge_conflict(#[case] staged: bool) {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        git(dir, &["switch", "--quiet", "-c", "other"]);
        write(dir, "a.txt", "other\n");
        git(dir, &["commit", "--quiet", "-am", "other"]);
        git(dir, &["switch", "--quiet", "-"]);
        write(dir, "a.txt", "mine\n");
        git(dir, &["commit", "--quiet", "-am", "mine"]);
        let merged = std::process::Command::new("git").arg("-C").arg(dir).args(["merge", "other"]).output();
        assert!(!merged.expect("merge").status.success());
        let error = stage(dir, "a.txt", digest(dir, "a.txt"), staged).expect_err("refused");
        assert!(error.to_string().contains("resolve the conflict first"), "{error}");
        assert_eq!(status(dir, &["a.txt"]), "UU a.txt\n");
    }
}

mod says {
    use super::*;

    #[rstest]
    #[case::staged("a.txt", None, true, "staged a.txt")]
    #[case::unstaged("a.txt", None, false, "unstaged a.txt")]
    #[case::renamed("new.txt", Some("old.txt"), false, "unstaged old.txt → new.txt")]
    fn what_went(#[case] path: &str, #[case] old: Option<&str>, #[case] staged: bool, #[case] text: &str) {
        assert_eq!(done(path, old, staged), text);
    }
}
