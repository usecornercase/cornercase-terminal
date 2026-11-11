use rstest::rstest;

use super::on_a_machine::start;
use super::parse;

#[rstest]
#[case::new_workspace(&["new-workspace", "b", "--worktree", "--fresh"])]
#[case::new_workspace_no_fresh(&["new-workspace", "b", "--worktree", "--no-fresh"])]
#[case::start(&["start", "--worktree", "b", "--no-fresh"])]
#[case::start_issue(&["start", "--issue", "7", "--fresh"])]
#[case::flow_run(&["flow", "run", "Quick", "--prompt", "p", "--fresh"])]
#[case::queue_add(&["queue", "add", "--at", "14:00", "--prompt", "x", "--worktree", "b", "--fresh"])]
#[case::queue_edit(&["queue", "edit", "3", "--no-fresh"])]
fn the_flags_go_where_a_branch_is_made(#[case] args: &[&str]) {
    parse(args).expect("parse");
}

#[rstest]
#[case::without_a_worktree(&["new-workspace", "b", "--fresh"])]
#[case::both(&["start", "--worktree", "b", "--fresh", "--no-fresh"])]
#[case::in_a_workspace(&["start", "--workspace", "3", "--fresh"])]
#[case::routine(&["queue", "add", "--every", "2h", "--prompt", "x", "--no-fresh"])]
#[case::queue_workspace(&["queue", "add", "--at", "14:00", "--workspace", "3", "--fresh"])]
fn the_flags_are_refused_where_they_cannot_mix(#[case] args: &[&str]) {
    let refused = parse(args).expect_err("wrong usage");

    assert_eq!(refused.exit_code(), 2, "{refused}");
}

#[test]
fn a_start_says_what_was_asked() {
    let fresh = start(&["start", "--worktree", "b", "--fresh"]).fresh.asked();
    let head = start(&["start", "--worktree", "b", "--no-fresh"]).fresh.asked();
    let neither = start(&["start", "--worktree", "b"]).fresh.asked();

    assert_eq!((fresh, head, neither), (Some(true), Some(false), None));
}
