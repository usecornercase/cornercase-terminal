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
    #[case::mark_what_runs_here(&["mark"])]
    #[case::mark_a_pane(&["mark", "--pane", "4"])]
    #[case::mark_a_tab_reviewed(&["mark", "--tab", "4", "--reviewed"])]
    #[case::move_a_pane(&["move-pane", "--pane", "4", "--target", "5", "--place", "left", "--json"])]
    #[case::restart_every_pane(&["account", "restart", "claude"])]
    #[case::restart_a_pane_on_an_account(&["account", "restart", "claude", "work", "--pane", "4", "--json"])]
    #[case::restart_a_pane_on_its_own_login(&["account", "restart", "claude", "--system", "--pane", "4"])]
    #[case::read_the_context_here(&["context"])]
    #[case::read_a_panes_context(&["context", "--pane", "4", "--json"])]
    #[case::read_a_tabs_cached_context(&["context", "--tab", "4", "--cached"])]
    #[case::collapse_a_group(&["collapse", "--group", "4"])]
    #[case::collapse_a_project(&["collapse", "--project", "4"])]
    #[case::collapse_a_workspace(&["collapse", "--workspace", "4"])]
    #[case::collapse_a_branch(&["collapse", "--worktree", "fix/login"])]
    #[case::collapse_an_agent(&["collapse", "--pane", "4"])]
    #[case::expand_a_group(&["expand", "--group", "4"])]
    #[case::expand_a_project(&["expand", "--project", "4"])]
    #[case::expand_a_workspace(&["expand", "--workspace", "4"])]
    #[case::expand_a_branch(&["expand", "--worktree", "fix/login"])]
    #[case::expand_an_agent(&["expand", "--pane", "4"])]
    #[case::send_here(&["send", "x"])]
    #[case::send_to_a_pane(&["send", "--pane", "4", "--enter", "x"])]
    #[case::send_to_a_tab_and_wait(&["send", "--tab", "4", "--enter", "--wait", "--until", "turn-over", "--timeout", "60", "x"])]
    #[case::send_standard_input(&["send", "--pane", "4", "--force", "--json", "-"])]
    #[case::input_into_a_pane(&["input", "--pane", "4", r#"{"paste": "x"}"#])]
    #[case::input_from_standard_input(&["input", "--pane", "4", "--json", "-"])]
    #[case::input_into_a_popup(&["input", "--popup", "--pane", "4", r#"{"key": {"code": "enter"}}"#])]
    #[case::paste_a_file(&["paste-file", "--pane", "4", "shot.png"])]
    #[case::paste_standard_input(&["paste-file", "--pane", "4", "--name", "x.png", "--json"])]
    fn goes_with_every_form_of_the_commands_that_take_it(#[case] args: &[&str]) {
        assert_eq!(boot_of(&[args, &["--boot", "1-2"]].concat()).as_deref(), Some("1-2"));
        assert_eq!(boot_of(args), None);
    }

    #[rstest]
    #[case::close(&["close", "--pane", "4", "--boot", ""])]
    #[case::rename(&["rename", "x", "--boot", ""])]
    #[case::mark(&["mark", "--pane", "4", "--boot", ""])]
    #[case::move_pane(&["move-pane", "--pane", "4", "--target", "5", "--place", "left", "--boot", ""])]
    #[case::account_restart(&["account", "restart", "claude", "--boot", ""])]
    #[case::context(&["context", "--boot", ""])]
    #[case::collapse(&["collapse", "--pane", "4", "--boot", ""])]
    #[case::expand(&["expand", "--group", "4", "--boot", ""])]
    #[case::send(&["send", "--pane", "4", "x", "--boot", ""])]
    #[case::input(&["input", "--pane", "4", r#"{"text": "x"}"#, "--boot", ""])]
    #[case::paste_file(&["paste-file", "--pane", "4", "--boot", ""])]
    fn cannot_be_empty_so_an_unset_variable_never_reaches_the_server(#[case] args: &[&str]) {
        assert!(parsed(args).is_err());
    }

    #[test]
    fn is_no_flag_of_the_commands_that_would_ignore_it() {
        assert!(parsed(&["status", "--boot", "1-2"]).is_err());
        assert!(parsed(&["keys", "--pane", "4", "enter", "--boot", "1-2"]).is_err());
        assert!(parsed(&["held", "--pane", "4", "--send-now", "--boot", "1-2"]).is_err());
    }

    #[test]
    fn is_a_usage_error_with_input_follow_rather_than_left_unchecked() {
        assert!(parsed(&["input", "--pane", "4", "--follow", "-", "--boot", "1-2"]).is_err());
        assert_eq!(boot_of(&["input", "--pane", "4", "--follow", "-"]), None);
    }
}
