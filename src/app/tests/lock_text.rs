use super::*;
use rstest::rstest;

use crate::git::worktree::{Holder, Lock};

fn lock(reason: &str, holder: Option<Holder>) -> Lock {
    Lock { reason: reason.into(), holder, role: None }
}

#[rstest]
#[case::with_a_reason(
    "busy",
    "The worktree of wt is locked: busy. Unlock it and delete its folder? The branch is kept."
)]
#[case::without_one("", "The worktree of wt is locked. Unlock it and delete its folder? The branch is kept.")]
fn the_message_names_the_reason(#[case] reason: &str, #[case] expected: &str) {
    assert_eq!(lock_message("wt", &lock(reason, None), None), expected);
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

#[test]
fn a_lock_on_a_role_worktree_names_the_role_and_its_folder_not_the_run() {
    let lock = Lock { role: Some(("coder A".into(), "/wt/coder-a".into())), ..lock("busy", None) };
    assert_eq!(
        lock_message("fix-login", &lock, None),
        "The worktree of `coder A` (/wt/coder-a) is locked: busy. Unlock it and remove fix-login with its worktrees? \
         The branches are kept."
    );
}

#[rstest]
#[case::of_its_own(
    None,
    "The worktree of api on vps is locked: busy. Unlock it and delete its folder? The branch is kept."
)]
#[case::of_a_role(
    Some(("coder A".into(), "/wt/coder-a".into())),
    "The worktree of `coder A` (/wt/coder-a) is locked: busy. Unlock it and remove api on vps with its worktrees? \
     The branches are kept."
)]
fn the_message_names_the_machine_of_a_remote_workspace(
    #[case] role: Option<(String, PathBuf)>,
    #[case] expected: &str,
) {
    let lock = Lock { role, ..lock("busy", None) };
    assert_eq!(lock_message("api", &lock, Some("vps")), expected);
}
