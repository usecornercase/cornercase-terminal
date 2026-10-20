use std::path::{Path, PathBuf};

use crate::test_util::{TempDir, git, git_repo};
use crate::worktrees::discover::{checked_out, discover};
use crate::worktrees::remove::{delete_branch, remove};
use crate::worktrees::{Found, Kind};

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
