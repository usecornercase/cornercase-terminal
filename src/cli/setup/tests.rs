use rstest::rstest;

use crate::cli::args::setup::SetupArgs;
use crate::cli::args::{Command, Control};
use crate::cli::tests::parse;

fn parsed(args: &[&str]) -> SetupArgs {
    let cli = parse(&[&["setup"], args].concat()).expect("parse");
    let Some(Command::Control(Control::Setup(setup))) = cli.command else { panic!("not a setup") };
    setup
}

#[test]
fn it_takes_a_workspace_and_what_new_tab_takes_to_show_and_print_its_tab() {
    let setup = parsed(&["--workspace", "16", "--focus", "--view", "3", "--json"]);

    assert_eq!(setup.workspace, Some(16));
    assert_eq!((setup.create.focus, setup.create.view.view, setup.create.print.json), (true, Some(3), true));
}

#[test]
fn it_takes_a_branch_or_leaves_the_workspace_to_where_it_runs() {
    assert_eq!(parsed(&["--worktree", " feat/x "]).worktree.as_deref(), Some("feat/x"));
    let here = parsed(&[]);
    assert_eq!((here.workspace, here.worktree), (None, None));
}

#[rstest]
#[case::a_workspace_and_a_branch(&["setup", "--workspace", "16", "--worktree", "feat/x"])]
#[case::an_empty_branch(&["setup", "--worktree", " "])]
fn it_refuses_what_names_no_one_workspace(#[case] args: &[&str]) {
    assert!(parse(args).is_err());
}

#[test]
fn the_command_says_which_view_it_acts_in() {
    let cli = parse(&["setup", "--view", "3"]).expect("parse");
    let Some(Command::Control(control)) = cli.command else { panic!("not a command for the server") };

    assert_eq!(control.view(), Some(3));
}
