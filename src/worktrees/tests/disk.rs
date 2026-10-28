use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::test_util::TempDir;
use crate::worktrees::disk::usage;

fn blocks(path: &Path) -> u64 {
    std::fs::symlink_metadata(path).expect("metadata").blocks() * 512
}

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, vec![7_u8; bytes]).expect("write");
}

fn part(name: &str) -> Vec<PathBuf> {
    vec![PathBuf::from(name)]
}

#[test]
fn counts_files_and_folders_and_the_part_inside_the_build_folder() {
    let root = TempDir::new();
    write(&root.path().join("a.txt"), 40_000);
    write(&root.path().join("node_modules/x/b.js"), 80_000);
    let found = usage(root.path(), &part("node_modules"));
    let inside = blocks(&root.path().join("node_modules"))
        + blocks(&root.path().join("node_modules/x"))
        + blocks(&root.path().join("node_modules/x/b.js"));
    let total = blocks(root.path()) + blocks(&root.path().join("a.txt")) + inside;
    assert_eq!(found.parts, [inside]);
    assert_eq!(found.bytes, total);
    assert_eq!(found.nested.len(), 0);
}

#[test]
fn two_links_inside_the_part_count_once_there() {
    let root = TempDir::new();
    write(&root.path().join("node_modules/a"), 40_000);
    std::fs::hard_link(root.path().join("node_modules/a"), root.path().join("node_modules/b")).expect("link");
    let found = usage(root.path(), &part("node_modules"));
    let once = blocks(&root.path().join("node_modules/a"));
    assert_eq!(found.parts[0], blocks(&root.path().join("node_modules")) + once);
}

#[test]
fn a_link_with_its_other_end_outside_the_part_is_not_in_the_part() {
    let root = TempDir::new();
    write(&root.path().join("node_modules/a"), 40_000);
    std::fs::hard_link(root.path().join("node_modules/a"), root.path().join("kept")).expect("link");
    let found = usage(root.path(), &part("node_modules"));
    let file = blocks(&root.path().join("kept"));
    assert_eq!(found.parts[0], blocks(&root.path().join("node_modules")));
    assert_eq!(found.bytes, blocks(root.path()) + blocks(&root.path().join("node_modules")) + file);
}

#[test]
fn a_link_with_its_other_end_outside_the_root_is_counted_nowhere() {
    let root = TempDir::new();
    let elsewhere = TempDir::new();
    write(&elsewhere.path().join("store"), 40_000);
    std::fs::create_dir_all(root.path().join("node_modules")).expect("mkdir");
    let linked = std::fs::hard_link(elsewhere.path().join("store"), root.path().join("node_modules/pkg")).is_ok();
    let found = usage(root.path(), &part("node_modules"));
    assert_eq!(found.parts[0], blocks(&root.path().join("node_modules")));
    assert_eq!(found.bytes, blocks(root.path()) + blocks(&root.path().join("node_modules")));
    assert!(linked || found.bytes > 0);
}

#[test]
fn a_symbolic_link_to_a_big_folder_adds_only_the_link() {
    let root = TempDir::new();
    let big = TempDir::new();
    write(&big.path().join("huge"), 400_000);
    std::os::unix::fs::symlink(big.path(), root.path().join("link")).expect("symlink");
    let found = usage(root.path(), &[]);
    assert_eq!(found.bytes, blocks(root.path()) + blocks(&root.path().join("link")));
}

#[test]
fn a_git_checkout_inside_a_part_is_reported() {
    let root = TempDir::new();
    write(&root.path().join(".venv/src/foo/.git/HEAD"), 10);
    write(&root.path().join("src/bar/.git/HEAD"), 10);
    let found = usage(root.path(), &part(".venv"));
    assert_eq!(found.nested, [PathBuf::from(".venv/src/foo/.git")]);
}
