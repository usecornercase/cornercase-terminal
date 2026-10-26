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
