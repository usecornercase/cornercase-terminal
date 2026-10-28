use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::test_util::{TempDir, git, git_repo};

use super::{Kind, Row, discover::discover, scan::branch_commits, scan::scan};

fn main_repo() -> TempDir {
    let repo = git_repo(&[("a.txt", "a\n")]);
    git(repo.path(), &["branch", "-M", "main"]);
    repo
}

fn add(repo: &Path, branch: &str, at: &Path) {
    git(repo, &["worktree", "add", "--quiet", "-b", branch, &at.display().to_string()]);
}

fn commit_at(dir: &Path, date: &str) {
    std::fs::write(dir.join("dated.txt"), date).expect("write");
    git(dir, &["add", "--all"]);
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "dated"])
        .env("GIT_COMMITTER_DATE", date)
        .env("GIT_AUTHOR_DATE", date)
        .output()
        .expect("run git");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

mod builds;
mod dialog;
mod disk;
mod list;
mod remove;

mod discover {
    use super::*;

    #[test]
    fn lists_linked_worktrees_of_open_repos_but_not_the_main_checkout() {
        let repo = main_repo();
        let dirs = TempDir::new();
        add(repo.path(), "feat-a", &dirs.path().join("a"));
        add(repo.path(), "feat-b", &dirs.path().join("b"));
        let empty = TempDir::new();
        let found = discover(&[repo.path().to_path_buf()], empty.path());
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.kind == Kind::Linked && f.admin.is_some()));
        let mut branches: Vec<_> = found.iter().filter_map(|f| f.branch.clone()).collect();
        branches.sort();
        assert_eq!(branches, ["feat-a", "feat-b"]);
    }

    #[test]
    fn finds_closed_repos_through_the_worktrees_folder() {
        let parent = TempDir::new();
        let repo = parent.path().join("repo");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
        let dir = TempDir::new();
        let inside = dir.path().join("repo").join("feat-a");
        std::fs::create_dir(dir.path().join("repo")).expect("mkdir");
        add(&repo, "feat-a", &inside);
        let outside = TempDir::new();
        add(&repo, "feat-b", &outside.path().join("b"));
        let found = discover(&[], dir.path());
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.project == "repo" && f.kind == Kind::Linked));
    }

    #[test]
    fn a_worktree_is_listed_once_through_a_symlinked_folder() {
        let repo = main_repo();
        let dir = TempDir::new();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("repo")).expect("mkdir");
        add(repo.path(), "feat-a", &real.join("repo").join("feat-a"));
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        let found = discover(&[repo.path().to_path_buf()], &link);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_missing_folder_is_prunable() {
        let repo = main_repo();
        let dirs = TempDir::new();
        let at = dirs.path().join("a");
        add(repo.path(), "feat-a", &at);
        std::fs::remove_dir_all(&at).expect("remove");
        let found = discover(&[repo.path().to_path_buf()], dirs.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, Kind::Prunable);
        assert!(found[0].admin.is_some());
    }

    #[test]
    fn a_missing_folder_added_with_relative_paths_is_prunable_with_its_admin() {
        let repo = main_repo();
        let dirs = TempDir::new();
        let at = dirs.path().join("a");
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo.path())
            .args(["worktree", "add", "--quiet", "--relative-paths", "-b", "feat-a"])
            .arg(&at)
            .output()
            .expect("run git");
        if !out.status.success() {
            return;
        }
        std::fs::remove_dir_all(&at).expect("remove");
        let found = discover(&[repo.path().to_path_buf()], dirs.path());
        assert_eq!((found.len(), found[0].kind), (1, Kind::Prunable));
        assert!(found[0].admin.is_some());
    }

    #[test]
    fn a_folder_whose_repository_is_gone_is_an_orphan() {
        let parent = TempDir::new();
        let repo = parent.path().join("repo");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join("repo")).expect("mkdir");
        add(&repo, "feat-a", &dir.path().join("repo").join("feat-a"));
        std::fs::remove_dir_all(&repo).expect("remove repo");
        let found = discover(&[], dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, Kind::Orphan);
        assert_eq!(found[0].project, "repo");
    }

    #[test]
    fn a_folder_whose_git_folder_has_no_repository_is_unknown_not_gone() {
        let dir = TempDir::new();
        let admin = dir.path().join("admin");
        std::fs::create_dir_all(&admin).expect("mkdir");
        let folder = dir.path().join("repo").join("w");
        std::fs::create_dir_all(&folder).expect("mkdir");
        std::fs::write(folder.join(".git"), format!("gitdir: {}\n", admin.display())).expect("write");
        let found = discover(&[], dir.path());
        assert_eq!(found.iter().map(|f| f.kind).collect::<Vec<_>>(), [Kind::Unknown]);
    }

    #[test]
    fn a_checkout_with_a_separate_git_folder_is_not_an_orphan() {
        let dir = TempDir::new();
        let folder = dir.path().join("repo").join("w");
        std::fs::create_dir_all(&folder).expect("mkdir");
        let store = dir.path().join("store.git");
        git(&folder, &["init", "--quiet", "--separate-git-dir", &store.display().to_string()]);
        assert!(folder.join(".git").is_file());
        assert_eq!(discover(&[], dir.path()), Vec::new());
    }

    #[test]
    fn a_symlinked_folder_in_the_worktrees_folder_is_not_followed() {
        let parent = TempDir::new();
        let repo = parent.path().join("repo");
        std::fs::create_dir(&repo).expect("mkdir");
        git(&repo, &["init", "--quiet"]);
        git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
        let elsewhere = TempDir::new();
        add(&repo, "feat-a", &elsewhere.path().join("repo").join("feat-a"));
        std::fs::remove_dir_all(&repo).expect("remove repo");
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join("repo")).expect("mkdir");
        let target = elsewhere.path().join("repo").join("feat-a");
        std::os::unix::fs::symlink(&target, dir.path().join("repo").join("feat-a")).expect("symlink");
        std::os::unix::fs::symlink(elsewhere.path().join("repo"), dir.path().join("linked")).expect("symlink");
        assert_eq!(discover(&[], dir.path()), Vec::new());
        assert_eq!(discover(&[], elsewhere.path()).len(), 1);
    }
}

mod scan {
    use super::*;
    use crate::worktrees::scan::{Panes, outside};

    fn worktree(repo: &Path, branch: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new();
        let at = dir.path().join("w");
        add(repo, branch, &at);
        (dir, at)
    }

    #[test]
    fn last_changed_is_the_newer_of_the_commit_and_an_untracked_file() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        let commit = SystemTime::UNIX_EPOCH + Duration::from_hours(438_288);
        assert_eq!(scan(&at, Some("main")).expect("scan").last_changed, Some(commit));
        std::fs::write(at.join("new.txt"), "x").expect("write");
        let mtime = std::fs::metadata(at.join("new.txt")).and_then(|m| m.modified()).expect("mtime");
        let found = scan(&at, Some("main")).expect("scan").last_changed.expect("a date");
        let gap = found.max(mtime).duration_since(found.min(mtime)).expect("gap");
        assert!(gap < Duration::from_secs(2));
    }

    #[test]
    fn counts_changes_and_unpushed_commits_without_a_remote() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        commit_at(&at, "2020-01-02T00:00:00Z");
        std::fs::write(at.join("a.txt"), "changed\n").expect("write");
        let scanned = scan(&at, Some("main")).expect("scan");
        assert_eq!(scanned.changed, 1);
        assert_eq!(scanned.unpushed, 2);
        assert!(!scanned.upstream_gone);
    }

    #[test]
    fn a_pushed_branch_has_nothing_unpushed() {
        let remote = main_repo();
        let (_clone_dir, clone) = crate::test_util::clone_of(remote.path());
        let (_dir, at) = worktree(&clone, "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        git(&at, &["push", "--quiet", "origin", "feat"]);
        assert_eq!(scan(&at, Some("origin/main")).expect("scan").unpushed, 0);
    }

    #[test]
    fn an_upstream_deleted_on_the_remote_is_gone() {
        let remote = main_repo();
        let (_clone_dir, clone) = crate::test_util::clone_of(remote.path());
        let (_dir, at) = worktree(&clone, "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        git(&at, &["push", "--quiet", "-u", "origin", "feat"]);
        git(remote.path(), &["branch", "-D", "feat"]);
        git(&at, &["fetch", "--quiet", "--prune"]);
        let scanned = scan(&at, Some("origin/main")).expect("scan");
        assert_eq!((scanned.upstream_gone, scanned.unpushed), (true, 1));
    }

    #[test]
    fn a_fresh_branch_is_not_merged_but_has_no_commits() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        let scanned = scan(&at, Some("main")).expect("scan");
        assert_eq!((scanned.merged, scanned.has_upstream, scanned.unpushed), (true, false, 0));
        let row = Row { scan: Some(Ok(scanned)), ..list::row("p", "feat", Kind::Linked) };
        assert_eq!((row.merged(), row.no_commits(), row.band()), (false, true, 2));
    }

    #[test]
    fn a_pushed_branch_in_the_base_is_merged() {
        let remote = main_repo();
        let (_clone_dir, clone) = crate::test_util::clone_of(remote.path());
        let (_dir, at) = worktree(&clone, "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        git(&at, &["push", "--quiet", "-u", "origin", "feat"]);
        git(remote.path(), &["merge", "--quiet", "--ff-only", "feat"]);
        git(&at, &["fetch", "--quiet"]);
        let scanned = scan(&at, Some("origin/main")).expect("scan");
        assert_eq!((scanned.merged, scanned.has_upstream), (true, true));
        let row = Row { scan: Some(Ok(scanned)), ..list::row("p", "feat", Kind::Linked) };
        assert_eq!((row.merged(), row.no_commits(), row.band()), (true, false, 1));
    }

    #[test]
    fn names_the_branch_checked_out_now_and_none_when_detached() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        assert_eq!(scan(&at, Some("main")).expect("scan").branch.as_deref(), Some("feat"));
        git(&at, &["checkout", "--quiet", "-b", "other"]);
        assert_eq!(scan(&at, Some("main")).expect("scan").branch.as_deref(), Some("other"));
        git(&at, &["checkout", "--quiet", "--detach"]);
        assert_eq!(scan(&at, Some("main")).expect("scan").branch, None);
    }

    #[test]
    fn counts_the_commits_only_a_branch_has() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        commit_at(&at, "2020-01-02T00:00:00Z");
        let counted = branch_commits(repo.path(), "feat", Some("main")).expect("count");
        assert_eq!((counted.unpushed, counted.branch.as_deref()), (2, Some("feat")));
        git(repo.path(), &["merge", "--quiet", "--ff-only", "feat"]);
        assert_eq!(branch_commits(repo.path(), "feat", Some("main")).expect("count").unpushed, 0);
        assert_eq!(branch_commits(repo.path(), "gone", Some("main")).expect("count").unpushed, 0);
        assert!(branch_commits(repo.path(), "-D", None).is_err());
    }

    #[test]
    fn merged_when_the_branch_is_in_the_base() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        assert!(!scan(&at, Some("main")).expect("scan").merged);
        git(repo.path(), &["merge", "--quiet", "--ff-only", "feat"]);
        assert!(scan(&at, Some("main")).expect("scan").merged);
        assert!(!scan(&at, None).expect("scan").merged);
    }

    #[test]
    fn a_failing_rev_list_fails_the_scan_instead_of_counting_nothing_unpushed() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        commit_at(&at, "2020-01-01T00:00:00Z");
        let remotes = repo.path().join(".git/refs/remotes/origin");
        std::fs::create_dir_all(&remotes).expect("mkdir");
        std::fs::write(remotes.join("gone"), "1234567890123456789012345678901234567890\n").expect("write ref");
        assert!(scan(&at, Some("main")).is_err());
    }

    #[test]
    fn a_folder_that_is_no_longer_a_worktree_fails_to_scan() {
        let dir = TempDir::new();
        assert!(scan(dir.path(), Some("main")).is_err());
    }

    #[test]
    fn finds_a_process_working_in_a_worktree_unless_its_own_workspace_runs_it() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        let sleeper =
            crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&at));
        let paths = vec![repo.path().to_path_buf(), at.clone()];
        let pid = sleeper.pid();
        let sleeping = |panes: &Panes| -> Vec<bool> {
            outside(&paths, panes, 0)
                .into_iter()
                .filter(|(i, name, _)| *i == 1 && name == "sleep")
                .map(|s| s.2)
                .collect()
        };
        crate::test_util::wait_until("the sleeper is named sleep", || sleeping(&Panes::default()) == [false]);
        assert_eq!(sleeping(&Panes::from([(pid, vec![1])])), Vec::<bool>::new());
        assert_eq!(sleeping(&Panes::from([(pid, vec![0])])), [true]);
    }

    #[test]
    fn a_plugin_process_in_a_worktree_is_reported() {
        let repo = main_repo();
        let (_dir, at) = worktree(repo.path(), "feat");
        let bin = TempDir::new();
        let git = bin.path().join("git");
        let copied = std::process::Command::new("/bin/sh")
            .args(["-c", "cp /bin/sleep \"$1\"", "sh"])
            .arg(&git)
            .status()
            .expect("run cp");
        assert!(copied.success(), "copy sleep");
        let spawn = |program: &Path| {
            crate::test_util::Running::spawn(std::process::Command::new(program).arg("30").current_dir(&at))
        };
        let (_plugin, _git) = (spawn(Path::new("/bin/sleep")), spawn(&git));
        let paths = vec![at.clone()];
        let me = i32::try_from(std::process::id()).expect("a pid");
        let names = |panes: &Panes| -> Vec<String> {
            outside(&paths, panes, me).into_iter().map(|(_, name, _)| name).collect()
        };
        crate::test_util::wait_until("both are running", || {
            let all = outside(&paths, &Panes::default(), 0);
            all.iter().any(|(_, n, _)| n == "sleep") && all.iter().any(|(_, n, _)| n == "git")
        });
        let found = names(&Panes::default());
        assert!(found.contains(&"sleep".to_string()) && !found.contains(&"git".to_string()), "{found:?}");
        assert_eq!(names(&Panes::from([(me, vec![0])])), Vec::<String>::new());
    }
}
