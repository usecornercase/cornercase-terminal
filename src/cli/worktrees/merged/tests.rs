use std::path::Path;

use super::*;
use crate::control::BuildsFreed;
use crate::control::worktrees::{BuildFolder, FailedFolder};
use crate::text::MB;

fn freed(path: &str, bytes: u64) -> WorktreeFreed {
    let folders = vec![BuildFolder { path: "target".into(), bytes }];
    let freed = BuildsFreed { path: path.into(), bytes, folders, ..BuildsFreed::default() };
    WorktreeFreed { outcome: FREED.into(), why: None, freed }
}

fn would(path: &str, bytes: u64) -> WorktreeFreed {
    let mut entry = freed(path, bytes);
    entry.outcome = FREEABLE.into();
    entry.freed.check = true;
    entry
}

fn refused(path: &str, why: &str) -> WorktreeFreed {
    let freed = BuildsFreed { path: path.into(), ..BuildsFreed::default() };
    WorktreeFreed { outcome: REFUSED.into(), why: Some(why.into()), freed }
}

fn failed(path: &str, why: &str) -> WorktreeFreed {
    let freed = BuildsFreed { path: path.into(), ..BuildsFreed::default() };
    WorktreeFreed { outcome: FAILED.into(), why: Some(why.into()), freed }
}

fn answer(check: bool, worktrees: Vec<WorktreeFreed>) -> MergedBuildsFreed {
    MergedBuildsFreed { check, bytes: worktrees.iter().map(|w| w.freed.bytes).sum(), worktrees }
}

fn show(answer: &MergedBuildsFreed) -> String {
    text(answer, Some(Path::new("/home/ana")))
}

#[test]
fn a_mixed_answer_names_each_worktree_and_sums_up() {
    let all = answer(
        false,
        vec![
            freed("/home/ana/wt/a", 300 * MB),
            freed("/home/ana/wt/b", 200 * MB),
            refused("/home/ana/wt/c", "cargo runs in it"),
            failed("/home/ana/wt/d", "not a worktree"),
        ],
    );

    assert_eq!(
        show(&all),
        "freed 300 MB in ~/wt/a: target 300 MB\nfreed 200 MB in ~/wt/b: target 200 MB\n\
did not free ~/wt/c: cargo runs in it\ncould not free ~/wt/d: not a worktree\nfreed 500 MB in 2 merged worktrees\n"
    );
}

#[test]
fn a_check_says_would() {
    let all = answer(true, vec![would("/home/ana/wt/a", 300 * MB), refused("/home/ana/wt/c", "cargo runs in it")]);

    assert_eq!(
        show(&all),
        "would free 300 MB in ~/wt/a: target 300 MB\nwould not free ~/wt/c: cargo runs in it\n\
would free 300 MB in 1 merged worktree\n"
    );
}

#[test]
fn an_empty_answer_says_nothing_has_builds() {
    assert_eq!(show(&answer(false, Vec::new())), "no merged worktree has builds to free\n");
}

#[test]
fn when_every_worktree_is_kept_nothing_was_freed() {
    let all = answer(false, vec![refused("/home/ana/wt/c", "cargo runs in it")]);
    assert!(show(&all).ends_with("nothing was freed\n"));
    let check = answer(true, vec![refused("/home/ana/wt/c", "cargo runs in it")]);
    assert!(show(&check).ends_with("nothing would be freed\n"));
}

#[test]
fn only_a_clean_free_went_well() {
    assert!(went_well(&freed("/a", 5)));
    assert!(went_well(&WorktreeFreed { outcome: FREEABLE.into(), ..freed("/a", 5) }));
    assert!(!went_well(&refused("/a", "x")));
    assert!(!went_well(&failed("/a", "x")));
    let mut some_failed = freed("/a", 5);
    some_failed.freed.failed = vec![FailedFolder { path: "target".into(), error: "busy".into() }];
    assert!(!went_well(&some_failed));
    let mut some_kept = freed("/a", 5);
    some_kept.freed.kept = vec!["target".into()];
    assert!(!went_well(&some_kept));
}
