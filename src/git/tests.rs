use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn repo_with_head(head: &str) -> TempDir {
    let tmp = TempDir::new();
    std::fs::create_dir(tmp.path().join(".git")).expect("create .git");
    std::fs::write(tmp.path().join(".git/HEAD"), head).expect("write HEAD");
    tmp
}

mod main_worktree {
    use super::*;

    #[test]
    fn is_the_folder_holding_the_git_dir() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let sub = repo.path().join("src");
        std::fs::create_dir(&sub).expect("create subfolder");
        assert_eq!(main_worktree(&sub), Some(repo.path().to_path_buf()));
    }

    #[test]
    fn is_none_inside_a_linked_worktree() {
        let tmp = TempDir::new();
        std::fs::write(tmp.path().join(".git"), "gitdir: /repo/.git/worktrees/wt\n").expect("write .git file");
        assert_eq!(main_worktree(tmp.path()), None);
    }

    #[test]
    fn is_none_outside_a_repo() {
        let tmp = TempDir::new();
        assert_eq!(main_worktree(tmp.path()), None);
    }
}

mod linked_worktrees {
    use super::*;
    use crate::test_util::{git, git_repo};

    #[test]
    fn lists_the_checkouts_made_with_git() {
        let repo = git_repo(&[]);
        let (a, b) = (TempDir::new(), TempDir::new());
        let (a_path, b_path) = (a.path().join("a"), b.path().join("b"));
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "b", &b_path.display().to_string()]);
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "a", &a_path.display().to_string()]);

        let mut expected = vec![a_path.canonicalize().expect("a"), b_path.canonicalize().expect("b")];
        expected.sort();
        assert_eq!(linked_worktrees(repo.path()), expected);
    }

    #[test]
    fn leaves_out_checkouts_whose_folder_is_gone() {
        let repo = git_repo(&[]);
        let tmp = TempDir::new();
        let path = tmp.path().join("gone");
        git(repo.path(), &["worktree", "add", "--quiet", "-b", "gone", &path.display().to_string()]);
        std::fs::remove_dir_all(&path).expect("remove worktree folder");

        assert_eq!(linked_worktrees(repo.path()), Vec::<PathBuf>::new());
    }

    #[test]
    fn is_empty_without_worktrees() {
        let repo = git_repo(&[]);
        assert_eq!(linked_worktrees(repo.path()), Vec::<PathBuf>::new());
    }
}

mod branch {
    use super::*;

    #[rstest]
    #[case::simple("ref: refs/heads/main\n", "main")]
    #[case::with_slashes("ref: refs/heads/feature/login\n", "feature/login")]
    #[case::detached_shows_short_sha("3f2a9c1d8e7b6a5f4e3d2c1b0a9f8e7d6c5b4a39\n", "3f2a9c1")]
    fn reads_head(#[case] head: &str, #[case] expected: &str) {
        let repo = repo_with_head(head);
        assert_eq!(branch(repo.path()).as_deref(), Some(expected));
    }

    #[test]
    fn is_found_from_a_subfolder() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let sub = repo.path().join("src/deep");
        std::fs::create_dir_all(&sub).expect("create subfolder");
        assert_eq!(branch(&sub).as_deref(), Some("main"));
    }

    #[test]
    fn follows_a_gitdir_file() {
        let repo = repo_with_head("ref: refs/heads/main\n");
        let worktrees = repo.path().join(".git/worktrees/wt");
        std::fs::create_dir_all(&worktrees).expect("create worktree gitdir");
        std::fs::write(worktrees.join("HEAD"), "ref: refs/heads/hotfix\n").expect("write HEAD");
        let wt = repo.path().join("wt");
        std::fs::create_dir(&wt).expect("create worktree");
        std::fs::write(wt.join(".git"), "gitdir: ../.git/worktrees/wt\n").expect("write .git file");
        assert_eq!(branch(&wt).as_deref(), Some("hotfix"));
    }

    #[test]
    fn is_none_outside_a_repo() {
        let tmp = TempDir::new();
        assert_eq!(branch(tmp.path()), None);
    }
}
