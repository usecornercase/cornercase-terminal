use super::*;
use rstest::rstest;

use crate::worktree::{Holder, Lock};

fn lock(reason: &str, holder: Option<Holder>) -> Lock {
    Lock { reason: reason.into(), holder }
}

#[rstest]
#[case::with_a_reason(
    "busy",
    "The worktree of wt is locked: busy. Unlock it and delete its folder? The branch is kept."
)]
#[case::without_one("", "The worktree of wt is locked. Unlock it and delete its folder? The branch is kept.")]
fn the_message_names_the_reason(#[case] reason: &str, #[case] expected: &str) {
    assert_eq!(lock_message("wt", &lock(reason, None)), expected);
}

#[rstest]
#[case::gone(Some(Holder::Gone(7)), false, Some("Process 7 is gone; the lock was left behind."))]
#[case::running(Some(Holder::Running(7)), false, Some("Process 7 still runs and may be using it."))]
#[case::nobody(None, false, None)]
#[case::nobody_with_changes(None, true, Some("It has uncommitted changes, which are deleted."))]
#[case::gone_with_changes(
    Some(Holder::Gone(7)),
    true,
    Some("Process 7 is gone; the lock was left behind. It has uncommitted changes, which are deleted.")
)]
fn the_note_says_who_holds_it(#[case] holder: Option<Holder>, #[case] changed: bool, #[case] expected: Option<&str>) {
    assert_eq!(lock_note(&lock("busy", holder), changed).as_deref(), expected);
}
