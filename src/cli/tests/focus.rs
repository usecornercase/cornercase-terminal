use rstest::rstest;

use super::parse;
use crate::cli::{Command, Control};

#[test]
fn a_reveal_takes_its_text_its_line_and_json() {
    let cli = parse(&["focus", "--pane", "12", "--reveal", "x y", "--line", "3", "--json"]).expect("parse");

    let Some(Command::Control(Control::Focus(focus))) = cli.command else { panic!("not focus") };
    assert_eq!((focus.reveal.as_deref(), focus.line, focus.print.json), (Some("x y"), Some(3), true));
}

#[rstest]
#[case::a_tab(&["focus", "--tab", "1", "--reveal", "x"])]
#[case::a_branch(&["focus", "--worktree", "b", "--reveal", "x"])]
#[case::a_line_alone(&["focus", "--pane", "1", "--line", "3"])]
fn wrong_usage_of_a_reveal_exits_with_2(#[case] args: &[&str]) {
    assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
}

#[test]
fn a_machine_takes_one_of_its_workspaces() {
    let cli = parse(&["focus", "--machine", "vps", "--workspace", "4", "--json"]).expect("parse");

    let Some(Command::Control(Control::Focus(focus))) = cli.command else { panic!("not focus") };
    assert_eq!((focus.machine.as_deref(), focus.which.workspace, focus.print.json), (Some("vps"), Some(4), true));
}

#[rstest]
#[case::nothing_of_it(&["focus", "--machine", "vps"])]
#[case::a_pane(&["focus", "--machine", "vps", "--pane", "1"])]
#[case::a_project(&["focus", "--machine", "vps", "--project", "1"])]
#[case::a_branch(&["focus", "--machine", "vps", "--worktree", "b"])]
#[case::a_view(&["focus", "--machine", "vps", "--tab", "1", "--view", "41"])]
#[case::a_reveal(&["focus", "--machine", "vps", "--tab", "1", "--reveal", "x"])]
fn wrong_usage_of_a_machine_exits_with_2(#[case] args: &[&str]) {
    assert_eq!(parse(args).expect_err("wrong usage").exit_code(), 2);
}
