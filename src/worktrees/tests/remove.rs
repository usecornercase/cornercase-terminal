use std::path::{Path, PathBuf};

use rstest::rstest;

use crate::git::worktree::RemoveOptions;
use crate::test_util::{TempDir, git, git_repo, link_folder};
use crate::worktrees::discover::{checked_out, discover};
use crate::worktrees::remove::{self, ALL, Named, delete_branch, named, still_listed};
use crate::worktrees::{Found, Kind};

fn remove(found: &Found, script: Option<&str>) -> crate::error::Result<Option<String>> {
    remove::remove(found, script, ALL)
}

fn main_repo() -> TempDir {
    let repo = git_repo(&[("a.txt", "a\n")]);
    git(repo.path(), &["branch", "-M", "main"]);
    repo
}

fn add(repo: &Path, branch: &str, at: &Path) {
    git(repo, &["worktree", "add", "--quiet", "-b", branch, &at.display().to_string()]);
}

fn only(repo: &Path, dirs: &Path) -> Found {
    let mut found = discover(&[repo.to_path_buf()], dirs);
    assert_eq!(found.len(), 1);
    found.remove(0)
}

fn branches(repo: &Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .expect("git");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn removes_a_closed_projects_worktree_and_keeps_its_branch() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    std::fs::write(at.join("dirty.txt"), "x").expect("write");
    let found = only(repo.path(), dirs.path());
    assert_eq!(remove(&found, None).expect("remove"), None);
    assert!(!at.exists());
    assert!(branches(repo.path()).contains("feat-a"));
}

#[test]
fn removes_a_locked_worktree() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    git(repo.path(), &["worktree", "lock", "--reason", "x", &at.display().to_string()]);
    let found = only(repo.path(), dirs.path());
    remove(&found, None).expect("remove");
    assert!(!at.exists());
}

#[cfg(unix)]
#[test]
fn runs_the_teardown_first() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    let marker = dirs.path().join("marker");
    let found = only(repo.path(), dirs.path());
    let script = format!("test -f a.txt && touch {}", marker.display());
    remove(&found, Some(&script)).expect("remove");
    assert!(marker.exists());
    assert!(!at.exists());
}

#[test]
fn prunes_only_the_entry_asked_for() {
    let repo = main_repo();
    let dirs = TempDir::new();
    for name in ["a", "b"] {
        add(repo.path(), &format!("feat-{name}"), &dirs.path().join(name));
        std::fs::remove_dir_all(dirs.path().join(name)).expect("remove");
    }
    let mut found = discover(&[repo.path().to_path_buf()], dirs.path());
    found.sort_by(|x, y| x.path.cmp(&y.path));
    assert!(found.iter().all(|f| f.kind == Kind::Prunable));
    remove(&found[0], None).expect("prune");
    let left = discover(&[repo.path().to_path_buf()], dirs.path());
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].path, found[1].path);
}

#[test]
fn does_not_prune_an_entry_that_is_no_longer_prunable() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    std::fs::remove_dir_all(&at).expect("remove");
    let found = only(repo.path(), dirs.path());
    add_back(&at, &found);
    assert!(remove(&found, None).is_err());
    assert!(repo.path().join(".git/worktrees").join(found.admin.as_deref().expect("admin")).exists());
}

fn add_back(at: &Path, found: &Found) {
    std::fs::create_dir_all(at).expect("mkdir");
    let admin = found.repo.join(".git/worktrees").join(found.admin.as_deref().expect("admin"));
    std::fs::write(at.join(".git"), format!("gitdir: {}\n", admin.display())).expect("write");
}

#[cfg(unix)]
#[test]
fn deletes_an_orphan_folder_but_not_through_a_symlink() {
    let dir = TempDir::new();
    let folder = dir.path().join("w");
    std::fs::create_dir(&folder).expect("mkdir");
    std::fs::write(folder.join(".git"), "gitdir: /nowhere/.git/worktrees/w\n").expect("write");
    std::fs::write(folder.join("file"), "x").expect("write");
    let orphan = |path: &Path| Found {
        repo: dir.path().join("gone"),
        project: "gone".to_string(),
        path: path.to_path_buf(),
        branch: None,
        admin: None,
        kind: Kind::Orphan,
    };
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&folder, &link).expect("symlink");
    assert!(remove(&orphan(&link), None).is_err());
    assert!(folder.join("file").exists());
    remove(&orphan(&folder), None).expect("remove");
    assert!(!folder.exists());
}

#[test]
fn refuses_an_orphan_that_is_not_a_worktree() {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("precious"), "x").expect("write");
    let found = Found {
        repo: dir.path().join("gone"),
        project: "gone".to_string(),
        path: dir.path().to_path_buf(),
        branch: None,
        admin: None,
        kind: Kind::Orphan,
    };
    assert!(remove(&found, None).is_err());
    assert!(dir.path().join("precious").exists());
}

#[test]
fn deletes_an_unmerged_branch() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    std::fs::write(at.join("n.txt"), "n").expect("write");
    git(&at, &["add", "--all"]);
    git(&at, &["-c", "user.name=t", "-c", "user.email=t@e.com", "commit", "--quiet", "-m", "n"]);
    git(repo.path(), &["worktree", "remove", "--force", &at.display().to_string()]);
    delete_branch(repo.path(), "feat-a").expect("delete");
    assert!(!branches(repo.path()).contains("feat-a"));
}

#[test]
fn refuses_to_delete_the_default_branch() {
    let repo = main_repo();
    git(repo.path(), &["branch", "other"]);
    git(repo.path(), &["checkout", "--quiet", "other"]);
    assert!(delete_branch(repo.path(), "main").is_err());
    assert!(delete_branch(repo.path(), "other").is_err());
    assert!(delete_branch(repo.path(), "-D").is_err());
    assert!(branches(repo.path()).contains("main"));
}

#[test]
fn refuses_a_branch_checked_out_in_another_worktree() {
    let repo = main_repo();
    let dirs = TempDir::new();
    add(repo.path(), "feat-a", &dirs.path().join("a"));
    assert!(delete_branch(repo.path(), "feat-a").is_err());
}

fn orphan_at(dir: &Path) -> Found {
    Found {
        repo: dir.join("gone"),
        project: "gone".to_string(),
        path: dir.join("w"),
        branch: None,
        admin: None,
        kind: Kind::Orphan,
    }
}

fn pointing_at_an_admin_folder(dir: &Path) -> PathBuf {
    let admin = dir.join("admin");
    std::fs::create_dir_all(&admin).expect("mkdir");
    std::fs::create_dir(dir.join("w")).expect("mkdir");
    std::fs::write(dir.join("w/.git"), format!("gitdir: {}\n", admin.display())).expect("write");
    admin
}

#[test]
fn refuses_an_orphan_whose_repository_came_back() {
    let dir = TempDir::new();
    pointing_at_an_admin_folder(dir.path());
    assert!(remove(&orphan_at(dir.path()), None).is_err());
    assert!(dir.path().join("w").exists());
    std::fs::write(dir.path().join("w/.git"), "gitdir: ../admin\n").expect("write");
    assert!(remove(&orphan_at(dir.path()), None).is_err());
    assert!(dir.path().join("w").exists());
}

#[test]
fn refuses_an_orphan_that_is_a_repository() {
    let dir = TempDir::new();
    std::fs::create_dir_all(dir.path().join("w/.git")).expect("mkdir");
    assert!(remove(&orphan_at(dir.path()), None).is_err());
    assert!(dir.path().join("w/.git").exists());
}

#[cfg(unix)]
#[test]
fn refuses_to_prune_through_a_symlinked_admin_folder() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    std::fs::remove_dir_all(&at).expect("remove");
    let found = only(repo.path(), dirs.path());
    let admin = repo.path().join(".git/worktrees").join(found.admin.as_deref().expect("admin"));
    let kept = dirs.path().join("kept");
    std::fs::rename(&admin, &kept).expect("move");
    std::os::unix::fs::symlink(&kept, &admin).expect("symlink");
    assert!(remove(&found, None).is_err());
    assert!(kept.join("gitdir").exists());
}

#[test]
fn removes_a_detached_worktree() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    git(repo.path(), &["worktree", "add", "--quiet", "--detach", &at.display().to_string()]);
    let found = only(repo.path(), dirs.path());
    assert_eq!(found.branch, None);
    remove(&found, None).expect("remove");
    assert!(!at.exists());
}

#[cfg(unix)]
#[test]
fn refuses_an_orphan_whose_gitdir_target_is_a_dangling_link() {
    let dir = TempDir::new();
    std::fs::create_dir(dir.path().join("w")).expect("mkdir");
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), &link).expect("symlink");
    std::fs::write(dir.path().join("w/.git"), format!("gitdir: {}\n", link.display())).expect("write");
    assert!(remove(&orphan_at(dir.path()), None).is_err());
    assert!(dir.path().join("w").exists());
}

#[test]
fn deletes_a_folder_git_no_longer_knows_but_not_once_it_is_known_again() {
    let dir = TempDir::new();
    let admin = pointing_at_an_admin_folder(dir.path());
    let unknown = Found { kind: Kind::Unknown, ..orphan_at(dir.path()) };
    std::fs::create_dir(admin.join("objects")).expect("mkdir");
    assert!(remove(&unknown, None).is_err());
    assert!(dir.path().join("w").exists());
    std::fs::remove_dir(admin.join("objects")).expect("rmdir");
    remove(&unknown, None).expect("remove");
    assert!(!dir.path().join("w").exists());
}

#[test]
fn reads_the_branch_checked_out_now_without_looking_above_the_folder() {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    let found = only(repo.path(), dirs.path());
    assert_eq!(checked_out(&found).as_deref(), Some("feat-a"));
    git(&at, &["checkout", "--quiet", "--detach"]);
    assert_eq!(checked_out(&found), None);
    git(&at, &["checkout", "--quiet", "feat-a"]);
    std::fs::remove_dir_all(&at).expect("remove");
    let prunable = only(repo.path(), dirs.path());
    assert_eq!((prunable.kind, checked_out(&prunable).as_deref()), (Kind::Prunable, Some("feat-a")));
    let inside = Found { path: repo.path().join("missing"), ..found };
    assert_eq!(checked_out(&inside), None);
}

fn one_worktree() -> (TempDir, TempDir, PathBuf) {
    let repo = main_repo();
    let dirs = TempDir::new();
    let at = dirs.path().join("a");
    add(repo.path(), "feat-a", &at);
    (repo, dirs, at)
}

fn why(asked: &Path, open: &[PathBuf], found: &[Found]) -> String {
    match named(asked, open, found) {
        Named::Unknown { why, .. } => why,
        Named::Listed(path) => panic!("{} was taken as {}", asked.display(), path.display()),
    }
}

mod naming_a_folder {
    use super::*;

    #[test]
    fn finds_a_listed_worktree_by_its_folder() {
        let (repo, dirs, at) = one_worktree();
        let found = [only(repo.path(), dirs.path())];

        assert_eq!(named(&at, &[], &found), Named::Listed(found[0].path.clone()));
    }

    #[test]
    fn finds_a_prunable_entry_whose_folder_is_gone() {
        let (repo, dirs, at) = one_worktree();
        std::fs::remove_dir_all(&at).expect("remove");
        let found = [only(repo.path(), dirs.path())];

        assert_eq!(found[0].kind, Kind::Prunable);
        assert_eq!(named(&at, &[], &found), Named::Listed(found[0].path.clone()));
    }

    #[test]
    fn refuses_a_relative_path_whatever_the_servers_folder_is() {
        let (repo, dirs, _at) = one_worktree();
        let found = [only(repo.path(), dirs.path())];

        assert_eq!(why(Path::new("."), &[], &found), "it is not an absolute path");
        assert_eq!(why(Path::new("a"), &[], &found), "it is not an absolute path");
    }

    #[test]
    fn calls_a_main_checkout_a_main_checkout_even_when_it_is_open() {
        let repo = main_repo();
        let open = [repo.path().to_path_buf()];

        let said = why(repo.path(), &open, &[]);

        assert!(said.contains("main checkout") && said.contains("never removed"), "{said}");
    }

    #[test]
    fn says_a_project_folder_is_a_projects_own() {
        let (_repo, _dirs, at) = one_worktree();

        assert_eq!(why(&at, std::slice::from_ref(&at), &[]), "it is a project's own folder");
    }

    #[test]
    fn says_a_plain_folder_a_folder_inside_a_worktree_and_nothing_at_all_are_not_worktrees() {
        let (repo, dirs, at) = one_worktree();
        let found = [only(repo.path(), dirs.path())];
        let inside = at.join("src");
        std::fs::create_dir(&inside).expect("mkdir");
        let unknown = "it is not a worktree that `cornercase worktrees list` knows";

        assert_eq!(why(dirs.path(), &[], &found), unknown);
        assert_eq!(why(&inside, &[], &found), unknown);
        assert_eq!(
            why(&dirs.path().join("nothing"), &[], &found),
            "nothing is there, and no repository lists a worktree there"
        );
    }

    #[test]
    fn takes_a_link_to_a_listed_folder_as_that_folder_and_refuses_a_link_to_any_other() {
        let (repo, dirs, at) = one_worktree();
        let found = [only(repo.path(), dirs.path())];
        let elsewhere = TempDir::new();
        let (to_listed, to_other) = (elsewhere.path().join("to-listed"), elsewhere.path().join("to-other"));
        link_folder(&at, &to_listed);
        link_folder(dirs.path(), &to_other);

        assert_eq!(named(&to_listed, &[], &found), Named::Listed(found[0].path.clone()));
        let refused = named(&to_other, &[], &found);
        assert!(matches!(&refused, Named::Unknown { path, .. } if path == dirs.path()), "{refused:?}");
    }

    #[test]
    fn refuses_a_linked_worktree_whose_record_cannot_be_read() {
        let (repo, dirs, at) = one_worktree();
        let found = [Found { admin: None, ..only(repo.path(), dirs.path()) }];

        assert!(why(&at, &[], &found).contains("cannot be read"));
    }

    #[test]
    fn refuses_a_worktree_git_records_at_a_link_since_git_would_delete_through_it() {
        let (repo, dirs, at) = one_worktree();
        let elsewhere = TempDir::new();
        let kept = elsewhere.path().join("kept");
        std::fs::rename(&at, &kept).expect("move");
        link_folder(&kept, &at);
        let found = discover(&[repo.path().to_path_buf()], dirs.path());
        assert_eq!((found.len(), found[0].path.as_path(), found[0].kind), (1, kept.as_path(), Kind::Linked));

        for asked in [&at, &kept] {
            let said = why(asked, &[], &found);
            assert!(said.contains("git records it at the symbolic link"), "{said}");
        }
    }
}

mod a_repository_whose_dot_git_is_a_file {
    use super::*;

    fn with_two_worktrees() -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new();
        let (repo, list) = (dir.path().join("repo"), dir.path().join("worktrees").join("repo"));
        let inside = repo.join(".gitdir");
        git(dir.path(), &["init", "--quiet", "--separate-git-dir", &inside.display().to_string(), "repo"]);
        git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
        add(&repo, "feat-a", &list.join("a"));
        add(&repo, "feat-b", &list.join("b"));
        (dir, list.join("a"), list.parent().expect("a parent").to_path_buf())
    }

    #[test]
    fn has_its_record_read_so_a_worktree_at_a_link_is_refused() {
        let (dir, at, list) = with_two_worktrees();
        let kept = dir.path().join("kept");
        std::fs::rename(&at, &kept).expect("move");
        link_folder(&kept, &at);
        let found = discover(&[], &list);
        assert!(found.iter().all(|row| row.admin.is_some()), "{found:?}");

        for asked in [&at, &kept] {
            let said = why(asked, &[], &found);
            assert!(said.contains("git records it at the symbolic link"), "{said}");
        }
    }

    #[test]
    fn has_its_worktree_found_and_a_prunable_entry_pruned() {
        let (_dir, at, list) = with_two_worktrees();
        assert_eq!(named(&at, &[], &discover(&[], &list)), Named::Listed(at.clone()));
        std::fs::remove_dir_all(&at).expect("remove");
        let found = discover(&[], &list);
        let entry = found.iter().find(|row| row.kind == Kind::Prunable).expect("the entry");

        assert_eq!(checked_out(entry).as_deref(), Some("feat-a"));
        remove(entry, None).expect("pruned");

        assert!(discover(&[], &list).iter().all(|row| row.kind != Kind::Prunable));
    }
}

mod a_folder_swapped_after_the_listing {
    use super::*;

    #[test]
    fn a_folder_as_it_was_and_a_folder_already_gone_are_still_listed() {
        let dir = TempDir::new();
        let folder = dir.path().join("w");
        std::fs::create_dir(&folder).expect("mkdir");

        still_listed(&folder).expect("the folder is as it was");
        still_listed(&dir.path().join("gone")).expect("nothing is there to follow");
    }

    #[test]
    fn a_linked_worktree_turned_into_a_link_is_not_removed_nor_torn_down_nor_is_its_target_touched() {
        let (repo, dirs, at) = one_worktree();
        let found = only(repo.path(), dirs.path());
        let kept = dirs.path().join("kept");
        std::fs::rename(&at, &kept).expect("move");
        link_folder(&kept, &at);

        let refused = remove::remove(&found, Some("echo ran > ../marker"), ALL).expect_err("refused");

        assert!(refused.to_string().contains("changed since it was listed"), "{refused}");
        assert!(kept.join("a.txt").exists() && !dirs.path().join("marker").exists());
    }

    #[test]
    fn an_orphan_under_a_folder_turned_into_a_link_is_not_deleted() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join("real/w")).expect("mkdir");
        std::fs::write(dir.path().join("real/w/.git"), "gitdir: /nowhere/.git/worktrees/w\n").expect("write");
        link_folder(&dir.path().join("real"), &dir.path().join("link"));
        let through_a_link = Found { path: dir.path().join("link").join("w"), ..orphan_at(dir.path()) };

        let refused = remove::remove(&through_a_link, None, ALL).expect_err("refused");

        assert!(refused.to_string().contains("changed since it was listed"), "{refused}");
        assert!(dir.path().join("real/w/.git").exists());
    }

    #[test]
    fn an_orphan_turned_into_a_link_is_not_deleted() {
        let dir = TempDir::new();
        std::fs::create_dir(dir.path().join("real")).expect("mkdir");
        std::fs::write(dir.path().join("real/.git"), "gitdir: /nowhere/.git/worktrees/w\n").expect("write");
        link_folder(&dir.path().join("real"), &dir.path().join("w"));

        let refused = remove::remove(&orphan_at(dir.path()), None, ALL).expect_err("refused");

        assert!(refused.to_string().contains("changed since it was listed"), "{refused}");
        assert!(dir.path().join("real/.git").exists());
    }
}

mod a_branch_at_the_commit_that_was_looked_at {
    use super::*;
    use crate::test_util::head;
    use crate::worktrees::remove::delete_branch_at;

    #[test]
    fn goes_when_it_has_not_moved_or_when_nobody_looked() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(repo.path(), &["worktree", "remove", "--force", &at.display().to_string()]);
        git(repo.path(), &["branch", "other"]);

        assert!(delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect("deleted"));
        assert!(delete_branch_at(repo.path(), "other", None).expect("deleted"));
        assert!(!branches(repo.path()).contains("feat-a") && !branches(repo.path()).contains("other"));
    }

    #[test]
    fn stays_when_it_moved_since() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(&at, &["commit", "--quiet", "--allow-empty", "-m", "later"]);
        git(repo.path(), &["worktree", "remove", "--force", &at.display().to_string()]);

        assert!(!delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect("kept"));
        assert!(branches(repo.path()).contains("feat-a"));
    }

    #[test]
    fn takes_its_settings_with_it() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(repo.path(), &["config", "branch.feat-a.remote", "origin"]);
        git(repo.path(), &["worktree", "remove", "--force", &at.display().to_string()]);

        assert!(delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect("deleted"));

        let config = std::fs::read_to_string(repo.path().join(".git").join("config")).expect("the config");
        assert!(!config.contains("feat-a"), "{config}");
    }

    #[test]
    fn leaves_the_branch_a_symbolic_ref_points_to() {
        let repo = main_repo();
        let tip = head(repo.path());
        git(repo.path(), &["branch", "target"]);
        git(repo.path(), &["symbolic-ref", "refs/heads/alias", "refs/heads/target"]);

        assert!(delete_branch_at(repo.path(), "alias", Some(&tip)).expect("deleted"));

        assert!(branches(repo.path()).contains("target") && !branches(repo.path()).contains("alias"));
    }

    #[test]
    fn stays_while_a_worktree_has_it_checked_out() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);

        let refused = delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect_err("refused");

        assert!(refused.to_string().contains("used by worktree at"), "{refused}");
        assert!(branches(repo.path()).contains("feat-a"));
    }

    #[rstest]
    #[case::a_rebase("rebase-merge/head-name", "refs/heads/feat-a\n")]
    #[case::an_older_rebase("rebase-apply/head-name", "refs/heads/feat-a\n")]
    #[case::a_rebase_updating_it("rebase-merge/update-refs", "refs/heads/feat-a\nabc\nabc\n")]
    #[case::a_bisect("BISECT_START", "feat-a\n")]
    fn stays_while_a_worktree_is_busy_with_it(#[case] file: &str, #[case] says: &str) {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(&at, &["checkout", "--quiet", "--detach"]);
        let admin = repo.path().join(".git").join("worktrees").join("a");
        std::fs::create_dir_all(admin.join(file).parent().expect("a parent")).expect("mkdir");
        std::fs::write(admin.join(file), says).expect("write");
        std::fs::write(admin.join("BISECT_LOG"), "").expect("write");

        let refused = delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect_err("refused");

        assert!(refused.to_string().contains("used by worktree at"), "{refused}");
        assert!(branches(repo.path()).contains("feat-a"));
    }

    #[test]
    fn goes_past_a_bisect_start_that_no_bisect_log_backs() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(&at, &["checkout", "--quiet", "--detach"]);
        let admin = repo.path().join(".git").join("worktrees").join("a");
        std::fs::write(admin.join("BISECT_START"), "feat-a\n").expect("write");

        assert!(delete_branch_at(repo.path(), "feat-a", Some(&tip)).expect("deleted"));
    }

    #[test]
    fn says_what_git_says_when_it_is_already_gone() {
        let (repo, _dirs, at) = one_worktree();
        let tip = head(&at);
        git(repo.path(), &["worktree", "remove", "--force", &at.display().to_string()]);
        git(repo.path(), &["branch", "-D", "feat-a"]);

        assert!(delete_branch_at(repo.path(), "feat-a", Some(&tip)).is_err());
    }
}

mod options {
    use super::*;

    const NONE: RemoveOptions = RemoveOptions { force: false, unlock: false };

    #[test]
    fn git_still_refuses_files_that_appeared_after_the_check_when_force_was_not_given() {
        let (repo, dirs, at) = one_worktree();
        std::fs::write(at.join("dirty.txt"), "x").expect("write");
        let found = only(repo.path(), dirs.path());

        assert!(remove::remove(&found, None, NONE).is_err());
        assert!(at.join("dirty.txt").exists());

        remove::remove(&found, None, RemoveOptions { force: true, unlock: false }).expect("force removes it");
        assert!(!at.exists());
    }

    #[test]
    fn git_still_refuses_a_lock_that_appeared_after_the_check_when_unlock_was_not_given() {
        let (repo, dirs, at) = one_worktree();
        git(repo.path(), &["worktree", "lock", "--reason", "x", &at.display().to_string()]);
        let found = only(repo.path(), dirs.path());

        assert!(remove::remove(&found, None, RemoveOptions { force: true, unlock: false }).is_err());
        assert!(at.join("a.txt").exists());

        remove::remove(&found, None, RemoveOptions { force: false, unlock: true }).expect("unlock removes it");
        assert!(!at.exists());
    }
}
