use clap::Parser as _;
use rstest::rstest;

use crate::cli::Cli;
use crate::cli::args::Command;

fn parsed(args: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied()))
}

fn boot_of(args: &[&str]) -> Option<String> {
    let cli = parsed(args).expect("parse");
    let Some(Command::Control(control)) = cli.command else { panic!("not a command for scripts") };
    control.boot().map(str::to_string)
}

mod boot {
    use super::*;

    #[rstest]
    #[case::a_pane(&["close", "--pane", "4"])]
    #[case::a_tab(&["close", "--tab", "4"])]
    #[case::a_workspace(&["close", "--workspace", "4"])]
    #[case::a_branch(&["close", "--worktree", "fix/login"])]
    #[case::a_project(&["close", "--project", "4"])]
    #[case::a_removal(&["close", "--workspace", "4", "--remove-worktree", "--force", "--unlock"])]
    #[case::a_check(&["close", "--workspace", "4", "--remove-worktree", "--check", "--json"])]
    #[case::an_agent(&["close", "--agent", "--pane", "4", "--check"])]
    #[case::rename_a_tab(&["rename", "--tab", "4", "x"])]
    #[case::rename_a_workspace(&["rename", "--workspace", "4", "x"])]
    #[case::rename_a_branch(&["rename", "--worktree", "fix/login", "x"])]
    #[case::rename_a_project(&["rename", "--project", "4", "x"])]
    #[case::rename_a_group(&["rename", "--group", "4", "x"])]
    #[case::rename_what_runs_here(&["rename", "x"])]
    fn goes_with_every_form_of_close_and_rename(#[case] args: &[&str]) {
        assert_eq!(boot_of(&[args, &["--boot", "1-2"]].concat()).as_deref(), Some("1-2"));
        assert_eq!(boot_of(args), None);
    }

    #[rstest]
    #[case::close(&["close", "--pane", "4", "--boot", ""])]
    #[case::rename(&["rename", "x", "--boot", ""])]
    fn cannot_be_empty_so_an_unset_variable_never_reaches_the_server(#[case] args: &[&str]) {
        assert!(parsed(args).is_err());
    }

    #[test]
    fn is_no_flag_of_the_commands_that_would_ignore_it() {
        assert!(parsed(&["status", "--boot", "1-2"]).is_err());
        assert!(parsed(&["send", "--pane", "4", "x", "--boot", "1-2"]).is_err());
    }
}
