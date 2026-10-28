use std::path::{Path, PathBuf};

use super::{add, main_repo};
use crate::test_util::{TempDir, git};
use crate::worktrees::Kind;
use crate::worktrees::builds::{deletable, free, listed, measure};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn defaults() -> Vec<String> {
    names(&["node_modules", "target", "dist", "build", "__pycache__"])
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, vec![7_u8; bytes]).expect("write");
}

fn worktree() -> (TempDir, TempDir, PathBuf) {
    let repo = main_repo();
    std::fs::write(
        repo.path().join(".gitignore"),
        "node_modules/\nbuild/\ndist/*\n!dist/keep.txt\n__pycache__/\ntarget\n.venv/\n",
    )
    .expect("ignore");
    write(&repo.path().join("build/tracked.txt"), 10);
    write(&repo.path().join("dist/keep.txt"), 10);
    git(repo.path(), &["add", "--all"]);
    git(repo.path(), &["add", "-f", "build/tracked.txt"]);
    git(repo.path(), &["commit", "--quiet", "-m", "ignore"]);
    let dirs = TempDir::new();
    let at = dirs.path().join("wt");
    add(repo.path(), "feat", &at);
    (repo, dirs, at)
}

fn sorted(mut found: Vec<PathBuf>) -> Vec<PathBuf> {
    found.sort();
    found
}

#[test]
fn lists_ignored_build_folders_at_any_depth() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/x.js"), 100);
    write(&at.join("packages/x/node_modules/y.js"), 100);
    write(&at.join("target/debug/z"), 100);
    write(&at.join("src/__pycache__/m.pyc"), 100);
    let found = sorted(listed(&at, &defaults()).expect("listed"));
    let expected: Vec<PathBuf> =
        ["node_modules", "packages/x/node_modules", "src/__pycache__", "target"].map(PathBuf::from).into();
    assert_eq!(found, expected);
}

#[test]
fn skips_what_git_tracks_or_does_not_ignore_or_is_not_on_the_list() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("build/out.js"), 100);
    write(&at.join("dist/bundle.js"), 100);
    write(&at.join("elsewhere/dist/plain.js"), 100);
    write(&at.join(".venv/lib/a"), 100);
    let real = at.join("real_modules");
    std::fs::create_dir_all(&real).expect("mkdir");
    std::os::unix::fs::symlink(&real, at.join("node_modules")).expect("symlink");
    let found = listed(&at, &defaults()).expect("listed");
    assert!(found.is_empty(), "{found:?}");
    let custom = listed(&at, &names(&[".venv"])).expect("listed");
    assert_eq!(custom, [PathBuf::from(".venv")]);
    assert_eq!(listed(&at, &[]).expect("listed"), Vec::<PathBuf>::new());
}

#[test]
fn measures_the_builds_part_and_the_total() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/x.js"), 200_000);
    let size = measure(&at, Kind::Linked, &defaults()).expect("measure");
    assert_eq!(size.builds.len(), 1);
    assert!(size.builds_bytes() >= 200_000);
    assert!(size.bytes > size.builds_bytes());
    let plain = measure(&at, Kind::Orphan, &defaults()).expect("measure");
    assert!(plain.builds.is_empty() && plain.bytes >= size.bytes);
}

fn status(at: &Path) -> String {
    let out =
        std::process::Command::new("git").arg("-C").arg(at).args(["status", "--porcelain"]).output().expect("git");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn freeing_deletes_only_the_chosen_ignored_folders_and_nothing_else() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/x.js"), 200_000);
    write(&at.join("build/out.js"), 100);
    write(&at.join("untracked.txt"), 10);
    std::fs::write(at.join("a.txt"), "edited\n").expect("edit");
    let before = status(&at);
    let size = measure(&at, Kind::Linked, &defaults()).expect("measure");
    let chosen: Vec<PathBuf> = size.builds.iter().map(|f| f.path.clone()).collect();
    let freed = free(&at, &defaults(), &chosen).expect("free");
    assert!(!at.join("node_modules").exists());
    assert!(at.join("build/tracked.txt").exists() && at.join("build/out.js").exists());
    assert!(at.join("untracked.txt").exists());
    assert_eq!(std::fs::read_to_string(at.join("a.txt")).expect("read"), "edited\n");
    assert_eq!(status(&at), before);
    assert!(freed.bytes() >= 200_000 && freed.failed.is_empty() && freed.kept.is_empty());
    let after = freed.after.expect("measured again");
    assert!(after.bytes < size.bytes && after.builds.is_empty());
}

#[test]
fn a_folder_git_tracks_after_the_listing_is_kept_and_named() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/x.js"), 100);
    let chosen = vec![PathBuf::from("node_modules")];
    git(&at, &["add", "-f", "node_modules/x.js"]);
    let freed = free(&at, &defaults(), &chosen).expect("free");
    assert!(at.join("node_modules/x.js").exists());
    assert_eq!(freed.kept, chosen);
    assert_eq!(freed.folders.len(), 0);
}

#[test]
fn a_folder_already_gone_is_kept_and_named() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/x.js"), 100);
    let chosen = vec![PathBuf::from("node_modules")];
    std::fs::remove_dir_all(at.join("node_modules")).expect("remove");
    let freed = free(&at, &defaults(), &chosen).expect("free");
    assert_eq!((freed.folders.len(), freed.failed.len()), (0, 0));
    assert_eq!(freed.kept, chosen);
}

#[test]
fn a_main_checkout_or_a_plain_folder_cannot_be_measured_as_linked() {
    let plain = TempDir::new();
    assert!(measure(plain.path(), Kind::Linked, &defaults()).is_err());
}

#[test]
fn a_symbolic_link_inside_a_build_folder_never_deletes_its_target() {
    let (_repo, _dirs, at) = worktree();
    let outside = TempDir::new();
    write(&outside.path().join("precious.txt"), 100);
    write(&at.join("node_modules/x.js"), 100);
    std::os::unix::fs::symlink(outside.path(), at.join("node_modules/link")).expect("symlink");
    let freed = free(&at, &defaults(), &[PathBuf::from("node_modules")]).expect("free");
    assert!(outside.path().join("precious.txt").exists());
    assert!(!at.join("node_modules").exists() && freed.failed.is_empty());
}

#[test]
fn a_build_folder_that_became_a_symbolic_link_is_kept() {
    let (_repo, _dirs, at) = worktree();
    let outside = TempDir::new();
    write(&outside.path().join("precious.txt"), 100);
    write(&at.join("node_modules/x.js"), 100);
    let chosen = vec![PathBuf::from("node_modules")];
    std::fs::remove_dir_all(at.join("node_modules")).expect("remove");
    std::os::unix::fs::symlink(outside.path(), at.join("node_modules")).expect("symlink");
    let freed = free(&at, &defaults(), &chosen).expect("free");
    assert_eq!(freed.kept, chosen);
    assert!(outside.path().join("precious.txt").exists());
}

#[test]
fn a_parent_that_became_a_symbolic_link_leading_out_is_kept() {
    let (_repo, _dirs, at) = worktree();
    let outside = TempDir::new();
    write(&outside.path().join("x/node_modules/precious.txt"), 100);
    write(&at.join("packages/x/node_modules/y.js"), 100);
    let chosen = vec![PathBuf::from("packages/x/node_modules")];
    std::fs::remove_dir_all(at.join("packages")).expect("remove");
    std::os::unix::fs::symlink(outside.path(), at.join("packages")).expect("symlink");
    let freed = free(&at, &defaults(), &chosen).expect("free");
    assert_eq!(freed.kept, chosen);
    assert!(outside.path().join("x/node_modules/precious.txt").exists());
}

#[test]
fn a_folder_on_another_device_is_refused_by_the_check() {
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/sub/x.js"), 100);
    assert!(super::super::disk::crosses_device(&at.join("node_modules"), 0));
    assert!(!super::super::disk::crosses_device(
        &at.join("node_modules"),
        std::os::unix::fs::MetadataExt::dev(&std::fs::metadata(&at).expect("meta"))
    ));
}

#[test]
fn a_folder_that_cannot_be_read_counts_as_crossing() {
    use std::os::unix::fs::PermissionsExt;
    let (_repo, _dirs, at) = worktree();
    write(&at.join("node_modules/sub/x.js"), 100);
    let sub = at.join("node_modules/sub");
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).expect("lock");
    let readable = std::fs::read_dir(&sub).is_ok();
    let device = std::os::unix::fs::MetadataExt::dev(&std::fs::metadata(&at).expect("meta"));
    let crosses = super::super::disk::crosses_device(&at.join("node_modules"), device);
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).expect("unlock");
    assert!(readable || crosses);
}

#[test]
fn the_deletion_check_refuses_a_parent_that_leads_out_and_a_plain_folder_passes() {
    let (_repo, _dirs, at) = worktree();
    let outside = TempDir::new();
    write(&outside.path().join("x/node_modules/y"), 10);
    write(&at.join("node_modules/y"), 10);
    assert_eq!(deletable(&at, &at.join("node_modules")), Ok(()));
    std::os::unix::fs::symlink(outside.path(), at.join("linked")).expect("symlink");
    assert!(deletable(&at, &at.join("linked/x/node_modules")).is_err());
}
