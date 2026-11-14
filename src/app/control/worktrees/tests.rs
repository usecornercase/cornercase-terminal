use std::path::Path;

use super::*;
use crate::test_util::{TempDir, git, git_repo};
use crate::worktrees::builds::Size;

pub(super) fn linked_with_builds() -> (TempDir, TempDir, PathBuf) {
    let repo = git_repo(&[(".gitignore", "node_modules/\n"), ("a.txt", "a\n")]);
    let outer = TempDir::new();
    let path = canonical(outer.path()).join("wt");
    git(repo.path(), &["worktree", "add", "--quiet", "-b", "feat", &path.display().to_string()]);
    std::fs::create_dir_all(path.join("node_modules")).expect("folder");
    std::fs::write(path.join("node_modules/x.js"), vec![b'x'; 300_000]).expect("file");
    std::fs::write(path.join("mine.txt"), "mine\n").expect("file");
    (repo, outer, path)
}

pub(super) fn freeing(path: &Path, running: &[&str]) -> Freeing {
    let running = running.iter().map(ToString::to_string).collect();
    Freeing { path: path.to_path_buf(), names: vec!["node_modules".into()], running, shells: Vec::new() }
}

mod freeing_unless_risky {
    use super::*;

    #[test]
    fn it_deletes_the_ignored_build_folder_and_nothing_else() {
        let (_repo, _outer, path) = linked_with_builds();

        let freed = freeing(&path, &[]).unless_risky().expect("measured").expect("not risky");

        assert!(freed.bytes >= 300_000, "{}", freed.bytes);
        assert!(!path.join("node_modules").exists());
        assert!(path.join("mine.txt").exists());
        assert!(path.join(".git").exists());
    }

    #[test]
    fn a_program_in_the_worktree_keeps_its_builds_and_is_never_forced() {
        let (_repo, _outer, path) = linked_with_builds();

        let risky = freeing(&path, &["sleep"]).unless_risky().expect("measured").expect_err("risky");

        assert_eq!(risky.running, ["sleep"]);
        assert!(risky.size.builds_bytes() >= 300_000);
        assert!(path.join("node_modules/x.js").exists());
    }
}

mod settling {
    use super::*;

    #[test]
    fn a_check_measures_and_deletes_nothing() {
        let (_repo, _outer, path) = linked_with_builds();

        let would = freeing(&path, &[]).settle(true).expect("measured").expect("not risky");

        assert!(would.check && would.bytes >= 300_000, "{would:?}");
        assert_eq!(would.folders[0].path, "node_modules");
        assert!(path.join("node_modules/x.js").exists());
    }

    #[test]
    fn a_check_with_a_program_running_is_risky_too() {
        let (_repo, _outer, path) = linked_with_builds();

        let risky = freeing(&path, &["sleep"]).settle(true).expect("measured").expect_err("risky");

        assert_eq!(risky.running, ["sleep"]);
        assert!(path.join("node_modules/x.js").exists());
    }
}

mod why_it_was_kept {
    use super::*;

    fn risky(running: &[&str], nested: &[&str]) -> Risky {
        let nested = nested.iter().map(PathBuf::from).collect();
        let size = Size { bytes: 10, builds: Vec::new(), nested };
        Risky { running: running.iter().map(ToString::to_string).collect(), size }
    }

    #[test]
    fn one_program_runs_and_two_run() {
        assert_eq!(risky(&["cargo"], &[]).why(Path::new("/wt")), "cargo runs in it");
        assert_eq!(risky(&["cargo", "node"], &[]).why(Path::new("/wt")), "cargo, node run in it");
    }

    #[test]
    fn a_nested_checkout_is_named_from_the_worktree() {
        let nested = "/wt/.venv/src/foo/.git";

        let why = risky(&["cargo"], &[nested]).why(Path::new("/wt"));

        assert_eq!(why, "cargo runs in it, .venv/src/foo is a git checkout");
    }
}
