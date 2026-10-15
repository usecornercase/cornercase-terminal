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
    use crate::test_util::{exited_pid, this_pid};

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

        assert_eq!(lock(&path).expect("read lock"), Some(Lock { reason: "on a usb disk".into(), holder: None }));
    }

    #[test]
    fn lock_without_a_reason_has_an_empty_one() {
        let (repo, _tmp, path) = with_worktree();
        locked(repo.path(), &path, None);

        assert_eq!(lock(&path).expect("read lock"), Some(Lock { reason: String::new(), holder: None }));
    }

    #[test]
    fn a_pid_in_the_reason_that_is_gone_is_a_lock_left_behind() {
        let (repo, _tmp, path) = with_worktree();
        let pid = exited_pid();
        locked(repo.path(), &path, Some(&format!("claude session wt (pid {pid} start Wed Oct  7 09:10:42 2026)")));

        assert_eq!(lock(&path).expect("read lock").and_then(|l| l.holder), Some(Holder::Gone(pid)));
    }

    #[test]
    fn a_pid_in_the_reason_that_runs_holds_the_lock() {
        let (repo, _tmp, path) = with_worktree();
        let pid = this_pid();
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

        assert_eq!((path.exists(), ref_exists(repo.path(), "refs/heads/wt").expect("show-ref")), (false, true));
    }

    #[test]
    fn unlock_removes_a_checkout_whose_lock_is_already_gone() {
        let (repo, _tmp, path) = with_worktree();

        remove(repo.path(), &path, false, true).expect("remove");

        assert_eq!((path.exists(), ref_exists(repo.path(), "refs/heads/wt").expect("show-ref")), (false, true));
    }

    #[test]
    fn deletes_a_clean_checkout_and_keeps_the_branch() {
        let (repo, _tmp, path) = with_worktree();

        remove(repo.path(), &path, false, false).expect("remove");

        assert_eq!((path.exists(), ref_exists(repo.path(), "refs/heads/wt").expect("show-ref")), (false, true));
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

fn said(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("run git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn rev(dir: &Path, rev: &str) -> String {
    said(dir, &["rev-parse", rev])
}

fn bare_with_branch(branch: &str) -> (TempDir, TempDir, PathBuf, String) {
    let source = git_repo(&[("README", "main")]);
    git(source.path(), &["checkout", "--quiet", "-b", branch]);
    std::fs::write(source.path().join("README"), "branch").expect("write file");
    git(source.path(), &["commit", "--quiet", "--all", "-m", "on the branch"]);
    let commit = rev(source.path(), "HEAD");
    git(source.path(), &["checkout", "--quiet", "main"]);
    let remotes = TempDir::new();
    let bare = remotes.path().join("remote.git");
    git(remotes.path(), &["clone", "--quiet", "--bare", &source.path().display().to_string(), "remote.git"]);
    (source, remotes, bare, commit)
}

mod from_a_remote {
    use super::*;
    use crate::test_util::clone_of;

    #[test]
    fn tracks_a_branch_that_only_a_remote_has() {
        let (_source, _remotes, bare, commit) = bare_with_branch("feat/x");
        let (_clone, repo) = clone_of(&bare);
        let out = TempDir::new();
        let path = out.path().join("wt");

        let made = create(&repo, "feat/x", &path).expect("create worktree");

        assert_eq!(
            (made, rev(&path, "HEAD"), said(&repo, &["rev-parse", "--abbrev-ref", "feat/x@{upstream}"])),
            (Made::Tracking("origin/feat/x".into()), commit, "origin/feat/x".into())
        );
    }

    #[test]
    fn makes_a_new_branch_from_head_when_no_remote_has_it() {
        let (_source, _remotes, bare, _) = bare_with_branch("feat/x");
        let (_clone, repo) = clone_of(&bare);
        let out = TempDir::new();
        let path = out.path().join("wt");

        let made = create(&repo, "feat/y", &path).expect("create worktree");

        assert_eq!((made, rev(&path, "HEAD")), (Made::New, rev(&repo, "HEAD")));
    }

    #[test]
    fn asks_which_remote_when_several_have_it() {
        let (_source, _remotes, bare, _) = bare_with_branch("feat/x");
        let (_clone, repo) = clone_of(&bare);
        git(&repo, &["remote", "add", "up", &bare.display().to_string()]);
        git(&repo, &["fetch", "--quiet", "up"]);
        let out = TempDir::new();

        let err = create(&repo, "feat/x", &out.path().join("wt")).expect_err("ambiguous");

        assert_eq!(
            err.to_string(),
            "branch feat/x is on several remotes (origin, up); set checkout.defaultRemote to the one to track"
        );
    }

    #[test]
    fn follows_checkout_default_remote() {
        let (_source, _remotes, bare, _) = bare_with_branch("feat/x");
        let (_clone, repo) = clone_of(&bare);
        git(&repo, &["remote", "add", "up", &bare.display().to_string()]);
        git(&repo, &["fetch", "--quiet", "up"]);
        git(&repo, &["config", "checkout.defaultRemote", "up"]);
        let out = TempDir::new();

        let made = create(&repo, "feat/x", &out.path().join("wt")).expect("create worktree");

        assert_eq!(made, Made::Tracking("up/feat/x".into()));
    }
}

mod pinned {
    use super::*;
    use crate::test_util::clone_of;

    fn with_pull() -> (TempDir, TempDir, TempDir, PathBuf, String) {
        let (source, remotes, bare, commit) = bare_with_branch("feat/x");
        git(&bare, &["update-ref", "refs/pull/1/head", &commit]);
        git(&bare, &["update-ref", "-d", "refs/heads/feat/x"]);
        let (clone, repo) = clone_of(&bare);
        (source, remotes, clone, repo, commit)
    }

    fn pin(commit: &str) -> Pin {
        Pin { remote: "origin".into(), reference: "pull/1/head".into(), commit: commit.into() }
    }

    #[test]
    fn checks_out_the_pull_request_head_at_its_commit() {
        let (_source, _remotes, _clone, repo, commit) = with_pull();
        let out = TempDir::new();
        let path = out.path().join("wt");

        let made = create_pinned(&repo, "review/1", &path, &pin(&commit[..7])).expect("create worktree");

        assert_eq!(
            (
                made,
                current_branch(&path),
                rev(&path, "HEAD"),
                said(&path, &["rev-parse", "--abbrev-ref", "@{upstream}"])
            ),
            (Made::Fetched("origin pull/1/head".into()), Some("review/1".into()), commit, String::new())
        );
    }

    #[test]
    fn refuses_another_commit_and_leaves_no_branch() {
        let (_source, _remotes, _clone, repo, commit) = with_pull();
        let out = TempDir::new();
        let path = out.path().join("wt");

        let err = create_pinned(&repo, "review/1", &path, &pin("0000000")).expect_err("wrong commit");

        assert_eq!(
            (err.to_string(), path.exists(), said(&repo, &["branch", "--list", "review/1"])),
            (
                format!("pull/1/head on origin is at {commit}, not 0000000; nothing was checked out"),
                false,
                String::new()
            )
        );
    }

    #[test]
    fn reuses_a_branch_already_at_the_commit() {
        let (_source, _remotes, _clone, repo, commit) = with_pull();
        git(&repo, &["branch", "review/1", &commit]);
        let out = TempDir::new();

        let made = create_pinned(&repo, "review/1", &out.path().join("wt"), &pin(&commit)).expect("create worktree");

        assert_eq!(made, Made::Local);
    }

    #[test]
    fn refuses_a_branch_that_is_elsewhere() {
        let (_source, _remotes, _clone, repo, commit) = with_pull();
        git(&repo, &["branch", "review/1", "HEAD"]);
        let out = TempDir::new();

        let err = create_pinned(&repo, "review/1", &out.path().join("wt"), &pin(&commit)).expect_err("elsewhere");

        assert_eq!(err.to_string(), format!("branch review/1 already exists at {}, not {commit}", rev(&repo, "HEAD")));
    }
}

mod options {
    use super::*;

    #[rstest]
    #[case::branch(
        "--orphan",
        "origin",
        "pull/1/head",
        "the branch `--orphan` starts with `-`, which git would read as an option"
    )]
    #[case::remote(
        "x",
        "--upload-pack=touch",
        "pull/1/head",
        "the remote `--upload-pack=touch` starts with `-`, which git would read as an option"
    )]
    #[case::reference("x", "origin", "-q", "the ref `-q` starts with `-`, which git would read as an option")]
    #[case::destination(
        "x",
        "origin",
        "pull/1/head:refs/heads/main",
        "`pull/1/head:refs/heads/main` is not a ref to fetch"
    )]
    #[case::forced("x", "origin", "+pull/1/head", "`+pull/1/head` is not a ref to fetch")]
    fn are_refused_before_git_runs(
        #[case] branch: &str,
        #[case] remote: &str,
        #[case] reference: &str,
        #[case] expected: &str,
    ) {
        let repo = git_repo(&[]);
        let out = TempDir::new();
        let pin = Pin { remote: remote.into(), reference: reference.into(), commit: "abcdef0".into() };

        let err = create_pinned(repo.path(), branch, &out.path().join("wt"), &pin).expect_err("refused");

        assert_eq!(err.to_string(), expected);
    }

    #[test]
    fn a_commit_needs_seven_hex_digits() {
        let repo = git_repo(&[]);
        let out = TempDir::new();
        let pin = Pin { remote: "origin".into(), reference: "pull/1/head".into(), commit: "--all".into() };

        let err = create_pinned(repo.path(), "x", &out.path().join("wt"), &pin).expect_err("refused");

        assert_eq!(err.to_string(), "`--all` is not a commit: give at least 7 hexadecimal digits of its hash");
    }

    #[test]
    fn a_branch_starting_with_a_dash_is_refused() {
        let repo = git_repo(&[]);
        let out = TempDir::new();

        let err = create(repo.path(), "-b", &out.path().join("wt")).expect_err("refused");

        assert_eq!(err.to_string(), "the branch `-b` starts with `-`, which git would read as an option");
    }
}
