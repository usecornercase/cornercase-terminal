use super::super::tests::{freeing, linked_with_builds};
use super::*;
use crate::control::contract::Code;

fn freed(path: &Path, target: Target, check: bool) -> Option<WorktreeFreed> {
    worktree_freed(path, target, check)
}

#[test]
fn a_worktree_is_freed_with_its_folders() {
    let (_repo, _outer, path) = linked_with_builds();

    let entry = freed(&path, Ok(freeing(&path, &[])), false).expect("an entry");

    assert_eq!((entry.outcome.as_str(), entry.why), (FREED, None));
    assert_eq!(entry.freed.folders[0].path, "node_modules");
    assert!(!path.join("node_modules").exists() && path.join("mine.txt").exists());
}

#[test]
fn a_check_says_freeable_and_deletes_nothing() {
    let (_repo, _outer, path) = linked_with_builds();

    let entry = freed(&path, Ok(freeing(&path, &[])), true).expect("an entry");

    assert_eq!((entry.outcome.as_str(), entry.freed.check), (FREEABLE, true));
    assert!(entry.freed.bytes >= 300_000);
    assert!(path.join("node_modules/x.js").exists());
}

#[test]
fn a_program_keeps_the_builds_and_says_why() {
    let (_repo, _outer, path) = linked_with_builds();

    let entry = freed(&path, Ok(freeing(&path, &["sleep"])), false).expect("an entry");

    assert_eq!(entry.outcome, REFUSED);
    assert_eq!(entry.why.as_deref(), Some("sleep runs in it"));
    assert_eq!(entry.freed.running, ["sleep"]);
    assert!(path.join("node_modules/x.js").exists());
}

#[test]
fn a_worktree_with_no_build_folders_is_left_out_freed_or_kept() {
    let (_repo, _outer, path) = linked_with_builds();
    std::fs::remove_dir_all(path.join("node_modules")).expect("remove");

    assert_eq!(freed(&path, Ok(freeing(&path, &[])), false), None);
    assert_eq!(freed(&path, Ok(freeing(&path, &["sleep"])), false), None);
}

#[test]
fn a_refused_target_says_the_refusal() {
    let path = PathBuf::from("/nowhere");
    let refusal = Refusal::new(Code::Busy, "it is being removed");

    let entry = freed(&path, Err(refusal), false).expect("an entry");

    assert_eq!((entry.outcome.as_str(), entry.why.as_deref()), (REFUSED, Some("it is being removed")));
    assert_eq!(entry.freed.path, path);
}

#[test]
fn a_worktree_that_is_no_longer_one_failed() {
    let (_repo, _outer, path) = linked_with_builds();
    std::fs::remove_dir_all(&path).expect("remove");

    let entry = freed(&path, Ok(freeing(&path, &[])), false).expect("an entry");

    assert_eq!(entry.outcome, FAILED);
    assert!(entry.why.is_some());
}
