use std::path::Path;

use super::*;
use crate::test_util::{TempDir, git, git_repo};

fn linked_with_builds() -> (TempDir, TempDir, PathBuf) {
    let repo = git_repo(&[(".gitignore", "node_modules/\n"), ("a.txt", "a\n")]);
    let outer = TempDir::new();
    let path = canonical(outer.path()).join("wt");
    git(repo.path(), &["worktree", "add", "--quiet", "-b", "feat", &path.display().to_string()]);
    std::fs::create_dir_all(path.join("node_modules")).expect("folder");
    std::fs::write(path.join("node_modules/x.js"), vec![b'x'; 300_000]).expect("file");
    std::fs::write(path.join("mine.txt"), "mine\n").expect("file");
    (repo, outer, path)
}

fn freeing(path: &Path, running: &[&str]) -> Freeing {
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
