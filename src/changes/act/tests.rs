use rstest::rstest;

use super::*;
use crate::changes::tests::{digest, status, write};
use crate::test_util::{TempDir, git, git_repo};

const ACTIONS: [FileAction; 3] = [FileAction::Discard, FileAction::Stage, FileAction::Unstage];

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).expect("read")
}

mod refuses {
    use super::*;

    #[rstest]
    #[case::discard(FileAction::Discard)]
    #[case::stage(FileAction::Stage)]
    #[case::unstage(FileAction::Unstage)]
    fn a_file_changed_after_the_digest(#[case] action: FileAction) {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "a.txt", "two\n");
        let seen = digest(dir, "a.txt");
        write(dir, "a.txt", "three, longer\n");
        assert!(matches!(action.run(dir, "a.txt", seen).expect("run"), Outcome::Changed));
        assert_eq!(read(dir, "a.txt"), "three, longer\n");
        assert_eq!(status(dir, &["a.txt"]), " M a.txt\n");
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
        for action in ACTIONS {
            for path in ["../outside.txt", "nope.txt"] {
                assert!(matches!(action.run(&dir, path, 0).expect("run"), Outcome::Gone), "{path}");
            }
        }
        assert_eq!(read(outer.path(), "outside.txt"), "keep\n");
    }

    #[rstest]
    #[case::discard(FileAction::Discard)]
    #[case::stage(FileAction::Stage)]
    #[case::unstage(FileAction::Unstage)]
    fn a_held_index_lock_with_gits_message(#[case] action: FileAction) {
        let repo = git_repo(&[("a.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "a.txt", "two\n");
        let seen = digest(dir, "a.txt");
        write(dir, ".git/index.lock", "");
        let error = action.run(dir, "a.txt", seen).expect_err("locked");
        assert!(error.to_string().contains("index.lock"), "{error}");
        assert_eq!(read(dir, "a.txt"), "two\n");
    }
}

mod touches_only_its_path {
    use super::*;

    #[rstest]
    #[case::discard(FileAction::Discard)]
    #[case::stage(FileAction::Stage)]
    #[case::unstage(FileAction::Unstage)]
    fn a_name_with_glob_characters(#[case] action: FileAction) {
        let repo = git_repo(&[("star*.txt", "one\n"), ("starA.txt", "one\n")]);
        let dir = repo.path();
        write(dir, "star*.txt", "two\n");
        write(dir, "starA.txt", "two\n");
        let outcome = action.run(dir, "star*.txt", digest(dir, "star*.txt")).expect("run");
        assert!(matches!(outcome, Outcome::Done(_)));
        assert_eq!(read(dir, "starA.txt"), "two\n");
        assert_eq!(status(dir, &[":(literal)starA.txt"]), " M starA.txt\n");
    }

    #[rstest]
    #[case::discard(FileAction::Discard)]
    #[case::stage(FileAction::Stage)]
    #[case::unstage(FileAction::Unstage)]
    fn a_workspace_in_a_subfolder(#[case] action: FileAction) {
        let repo = git_repo(&[("proj/p.txt", "one\n"), ("top.txt", "one\n")]);
        let (dir, sub) = (repo.path(), repo.path().join("proj"));
        write(dir, "proj/p.txt", "two\n");
        write(dir, "top.txt", "two\n");
        let outcome = action.run(&sub, "p.txt", digest(&sub, "p.txt")).expect("run");
        assert!(matches!(outcome, Outcome::Done(_)));
        assert_eq!(read(dir, "top.txt"), "two\n");
        assert_eq!(status(dir, &["top.txt"]), " M top.txt\n");
    }
}

mod says {
    use super::*;

    #[test]
    fn what_each_action_is_called() {
        let names: Vec<_> = ACTIONS.iter().map(|a| (a.name(), a.past(), a.doing(), a.command())).collect();
        assert_eq!(
            names,
            [
                ("discard", "discarded", "discarding a file", "changes-discard"),
                ("stage", "staged", "staging a file", "changes-stage"),
                ("unstage", "unstaged", "unstaging a file", "changes-stage"),
            ]
        );
    }
}
