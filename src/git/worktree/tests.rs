use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, git, git_repo};

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn current_branch(dir: &Path) -> Option<String> {
    crate::git::branch(dir)
}

mod slug {
    use super::*;

    #[rstest]
    #[case::plain("login", "login")]
    #[case::slashes("feat/Login", "feat-login")]
    #[case::runs_of_symbols("fix//the  bug!", "fix-the-bug")]
    #[case::trims_dashes("-x-", "x")]
    #[case::nothing_left("///", "worktree")]
    fn turns_a_branch_into_a_folder_name(#[case] branch: &str, #[case] expected: &str) {
        assert_eq!(slug(branch), expected);
    }
}

#[test]
fn checkout_path_goes_under_the_repo_name() {
    let path = checkout_path(Path::new("/w"), Path::new("/home/ana/projects/cornercase"), "feat/login");
    assert_eq!(path, PathBuf::from("/w/cornercase/feat-login"));
}

mod create {
    use super::*;

    #[test]
    fn checks_out_a_new_branch() {
        let repo = git_repo(&[("README", "hi")]);
        let out = TempDir::new();
        let path = out.path().join("repo/login");

        create(repo.path(), "feat/login", &path).expect("create worktree");

        assert_eq!(
            (current_branch(&path).as_deref(), read(&path.join("README")).as_deref()),
            (Some("feat/login"), Some("hi"))
        );
    }

    #[test]
    fn create_from_branches_from_the_base_not_from_head() {
        let repo = git_repo(&[("README", "hi")]);
        git(repo.path(), &["checkout", "-q", "-b", "base"]);
        std::fs::write(repo.path().join("later"), "x").expect("write");
        git(repo.path(), &["add", "later"]);
        git(repo.path(), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "later"]);
        git(repo.path(), &["checkout", "-q", "-"]);
        let out = TempDir::new();
        let path = out.path().join("wt");

        create_from(repo.path(), "role", &path, "base").expect("create worktree");

        assert_eq!((current_branch(&path).as_deref(), read(&path.join("later")).as_deref()), (Some("role"), Some("x")));
    }

    #[test]
    fn checks_out_an_existing_branch() {
        let repo = git_repo(&[("README", "hi")]);
        git(repo.path(), &["branch", "already-there"]);
        let out = TempDir::new();
        let path = out.path().join("wt");

        create(repo.path(), "already-there", &path).expect("create worktree");

        assert_eq!(current_branch(&path).as_deref(), Some("already-there"));
    }

    #[test]
    fn copies_ignored_files_listed_in_worktreeinclude() {
        let repo = git_repo(&[
            (".gitignore", ".env\n.envs/\nsecret.txt\n"),
            (INCLUDE_FILE, ".env\n.envs/.env.local\n"),
            (".env", "TOKEN=1"),
            (".envs/.env.local", "LOCAL=1"),
            ("secret.txt", "not listed"),
        ]);
        let out = TempDir::new();
        let path = out.path().join("wt");

        create(repo.path(), "copy", &path).expect("create worktree");

        assert_eq!(
            (read(&path.join(".env")), read(&path.join(".envs/.env.local")), read(&path.join("secret.txt"))),
            (Some("TOKEN=1".into()), Some("LOCAL=1".into()), None)
        );
    }

    #[test]
    fn leaves_untracked_files_that_are_not_ignored() {
        let repo = git_repo(&[(INCLUDE_FILE, "notes.md\n")]);
        std::fs::write(repo.path().join("notes.md"), "draft").expect("write untracked file");
        let out = TempDir::new();
        let path = out.path().join("wt");

        create(repo.path(), "untracked", &path).expect("create worktree");

        assert_eq!(read(&path.join("notes.md")), None);
    }

    #[test]
    fn refuses_a_path_that_exists() {
        let repo = git_repo(&[]);
        let out = TempDir::new();

        let err = create(repo.path(), "x", out.path()).expect_err("path exists");

        assert!(matches!(err, Error::PathExists(_)), "{err:?}");
    }

    #[test]
    fn reports_what_git_says() {
        let repo = git_repo(&[]);
        let out = TempDir::new();

        let err = create(repo.path(), "not a branch", &out.path().join("wt")).expect_err("invalid branch");

        assert_eq!(err.to_string(), "'not a branch' is not a valid branch name");
    }
}

mod pid_in {
    use super::*;

    #[rstest]
    #[case::claude_code("claude session wt (pid 69184 start Wed Oct  7 09:10:42 2026)", Some(69184))]
    #[case::no_pid("on a usb disk", None)]
    #[case::pid_without_a_number("pid abc", None)]
    #[case::empty("", None)]
    fn finds_the_pid_a_reason_names(#[case] reason: &str, #[case] expected: Option<i32>) {
        assert_eq!(pid_in(reason), expected);
    }
}

mod remove {
    use super::*;

    fn with_worktree() -> (TempDir, TempDir, PathBuf) {
        let repo = git_repo(&[("README", "hi")]);
        let tmp = TempDir::new();
        let path = tmp.path().join("wt");
        create(repo.path(), "wt", &path).expect("create worktree");
        (repo, tmp, path)
    }

    fn locked(repo: &Path, path: &Path, reason: Option<&str>) {
        let path = path.display().to_string();
        match reason {
            Some(reason) => git(repo, &["worktree", "lock", "--reason", reason, &path]),
            None => git(repo, &["worktree", "lock", &path]),
        }
    }

    #[test]
    fn an_unlocked_checkout_has_no_lock() {
        let (_repo, _tmp, path) = with_worktree();

        assert_eq!(lock(&path).expect("read lock"), None);
    }

    #[test]
    fn lock_gives_the_reason() {
        let (repo, _tmp, path) = with_worktree();
        locked(repo.path(), &path, Some("on a usb disk"));

        assert_eq!(
            lock(&path).expect("read lock"),
            Some(Lock { reason: "on a usb disk".into(), holder: None, role: None })
        );
    }

    #[test]
    fn lock_without_a_reason_has_an_empty_one() {
        let (repo, _tmp, path) = with_worktree();
        locked(repo.path(), &path, None);

        assert_eq!(lock(&path).expect("read lock"), Some(Lock { reason: String::new(), holder: None, role: None }));
    }

    #[cfg(unix)]
    #[test]
    fn a_pid_in_the_reason_that_is_gone_is_a_lock_left_behind() {
        let (repo, _tmp, path) = with_worktree();
        let pid = crate::test_util::exited_pid();
        locked(repo.path(), &path, Some(&format!("claude session wt (pid {pid} start Wed Oct  7 09:10:42 2026)")));

        assert_eq!(lock(&path).expect("read lock").and_then(|l| l.holder), Some(Holder::Gone(pid)));
    }

    #[cfg(unix)]
    #[test]
    fn a_pid_in_the_reason_that_runs_holds_the_lock() {
        let (repo, _tmp, path) = with_worktree();
        let pid = crate::test_util::this_pid();
        locked(repo.path(), &path, Some(&format!("claude session wt (pid {pid} start …)")));

        assert_eq!(lock(&path).expect("read lock").and_then(|l| l.holder), Some(Holder::Running(pid)));
    }

    #[test]
    fn status_reports_changes_and_the_lock_together() {
        let (repo, _tmp, path) = with_worktree();
        std::fs::write(path.join("notes.txt"), "draft").expect("write file");
        locked(repo.path(), &path, Some("busy"));

        let status = status(&path);

        assert_eq!((status.changed, status.lock.map(|l| l.reason)), (true, Some("busy".into())));
    }

    #[test]
    fn a_locked_checkout_is_refused_without_unlock() {
        let (repo, _tmp, path) = with_worktree();
        locked(repo.path(), &path, Some("busy"));

        let result = remove(repo.path(), &path, true, false);

        assert!(matches!(result, Err(Error::Git(_))) && path.exists(), "{result:?}");
    }

    #[test]
    fn unlock_removes_a_locked_checkout_and_keeps_the_branch() {
        let (repo, _tmp, path) = with_worktree();
        locked(repo.path(), &path, Some("busy"));

        remove(repo.path(), &path, false, true).expect("remove");

        assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
    }

    #[test]
    fn unlock_removes_a_checkout_whose_lock_is_already_gone() {
        let (repo, _tmp, path) = with_worktree();

        remove(repo.path(), &path, false, true).expect("remove");

        assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
    }

    #[test]
    fn deletes_a_clean_checkout_and_keeps_the_branch() {
        let (repo, _tmp, path) = with_worktree();

        remove(repo.path(), &path, false, false).expect("remove");

        assert_eq!((path.exists(), branch_exists(repo.path(), "wt").expect("show-ref")), (false, true));
    }

    #[test]
    fn refuses_a_checkout_with_changes() {
        let (repo, _tmp, path) = with_worktree();
        std::fs::write(path.join("README"), "changed").expect("edit file");

        let result = remove(repo.path(), &path, false, false);

        assert!(matches!(result, Err(Error::Git(_))) && path.exists(), "{result:?}");
    }

    #[test]
    fn force_deletes_a_checkout_with_changes() {
        let (repo, _tmp, path) = with_worktree();
        std::fs::write(path.join("README"), "changed").expect("edit file");

        remove(repo.path(), &path, true, false).expect("remove");

        assert!(!path.exists());
    }

    #[rstest]
    #[case::nothing(None, false)]
    #[case::an_edited_file(Some("README"), true)]
    #[case::a_new_file(Some("notes.txt"), true)]
    fn changes_are_what_makes_git_refuse(#[case] written: Option<&str>, #[case] expected: bool) {
        let (_repo, _tmp, path) = with_worktree();
        if let Some(name) = written {
            std::fs::write(path.join(name), "changed").expect("write file");
        }

        assert_eq!(changed(&path).expect("git status"), expected);
    }
}

mod scoped_commands {
    use super::*;

    #[test]
    fn never_inherit_the_server_s_repository_variables() {
        let cmd = scoped(Path::new("/repo"), ["status"], &[("GIT_INDEX_FILE", OsStr::new("/scratch/index"))]);

        let envs: Vec<_> = cmd.get_envs().collect();
        for name in INHERITED.into_iter().filter(|name| *name != "GIT_INDEX_FILE") {
            assert!(envs.contains(&(OsStr::new(name), None)), "{name} is not removed: {envs:?}");
        }
        assert!(envs.contains(&(OsStr::new("GIT_INDEX_FILE"), Some(OsStr::new("/scratch/index")))));
    }

    #[test]
    fn every_runner_drops_them() {
        let cmd = command(Path::new("/repo"), ["status"]);

        let envs: Vec<_> = cmd.get_envs().collect();
        for name in INHERITED {
            assert!(envs.contains(&(OsStr::new(name), None)), "{name} is not removed: {envs:?}");
        }
    }

    #[test]
    fn config_injected_through_the_environment_is_dropped() {
        let names = ["GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_12", "GIT_CONFIG_GLOBAL", "PATH"];

        let dropped = injected_config(names.into_iter().map(OsString::from));

        assert_eq!(dropped, ["GIT_CONFIG_COUNT", "GIT_CONFIG_KEY_0", "GIT_CONFIG_VALUE_12"].map(OsString::from));
    }
}

#[cfg(not(unix))]
mod on_windows {
    use super::*;
    use crate::test_util::verbatim;

    #[test]
    fn a_checkout_is_made_locked_and_removed_through_verbatim_paths() {
        let repo = git_repo(&[("README", "hi"), (".worktreeinclude", ".env\n"), (".gitignore", ".env\n")]);
        std::fs::write(repo.path().join(".env"), "secret").expect("write .env");
        let tmp = TempDir::new();
        let path = tmp.path().join("año");

        create(&verbatim(repo.path()), "feat", &verbatim(&path)).expect("create");

        assert_eq!(read(&path.join("README")).as_deref(), Some("hi"));
        assert_eq!(read(&path.join(".env")).as_deref(), Some("secret"));
        let dot_git = read(&path.join(".git")).expect("the .git file");
        assert!(!dot_git.contains('?'), "{dot_git}");
        git(repo.path(), &["worktree", "lock", "--reason", "busy", &path.display().to_string()]);
        assert_eq!(lock(&verbatim(&path)).expect("lock").map(|lock| lock.reason), Some("busy".into()));
        remove(&verbatim(repo.path()), &verbatim(&path), false, true).expect("remove");
        assert!(!path.exists());
    }
}
