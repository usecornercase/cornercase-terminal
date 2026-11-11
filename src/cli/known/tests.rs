use clap::error::ErrorKind;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::cli::args::{Command as Run, RemoteArgs};
use crate::cli::tests::parse;

fn machine(destination: &str, command: Option<&str>) -> Machine {
    Machine { destination: destination.into(), command: command.map(Into::into) }
}

fn remote(args: &[&str]) -> RemoteArgs {
    let cli = parse(args).expect("parse");
    let Some(Run::Remote(remote)) = cli.command else { panic!("not a remote") };
    remote
}

mod its_arguments {
    use super::*;

    #[test]
    fn the_list_needs_no_destination() {
        let list = remote(&["remote", "--known"]);
        assert!(list.known && !list.json);
        assert_eq!(list.destination, None);
        assert!(remote(&["remote", "--known", "--json"]).json);
    }

    #[test]
    fn a_window_still_needs_its_destination() {
        let error = parse(&["remote"]).expect_err("no destination");
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("<DESTINATION>"));
        let window = remote(&["remote", "devbox", "--command", "cc"]);
        assert_eq!(
            (window.destination.as_deref(), window.command.as_deref(), window.known),
            (Some("devbox"), Some("cc"), false)
        );
    }

    #[rstest]
    #[case::a_destination(&["remote", "--known", "devbox"])]
    #[case::a_command(&["remote", "--known", "--command", "cc"])]
    fn refuse_what_means_nothing_for_the_list(#[case] args: &[&str]) {
        let error = parse(args).expect_err("conflict");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn json_needs_the_list() {
        let error = parse(&["remote", "devbox", "--json"]).expect_err("json alone");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
        let error = parse(&["remote", "--json"]).expect_err("json alone");
        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn show_both_forms_in_the_usage() {
        let text = parse(&["remote"]).expect_err("no destination").to_string();
        assert!(text.contains("cornercase remote [OPTIONS] <DESTINATION>"), "{text}");
        assert!(text.contains("cornercase remote --known [--json]"), "{text}");
    }
}

mod the_list {
    use super::*;

    #[test]
    fn prints_each_machine_with_its_command_or_null() {
        let known = [machine("devbox", None), machine("me@10.0.0.5", Some("~/bin/cornercase"))];
        assert_eq!(
            serde_json::to_value(Known::from(&known[..])).expect("json"),
            json!({"machines": [
                {"destination": "devbox", "command": null},
                {"destination": "me@10.0.0.5", "command": "~/bin/cornercase"},
            ]})
        );
    }

    #[test]
    fn an_empty_list_is_an_empty_array() {
        assert_eq!(serde_json::to_value(Known::from(&[][..])).expect("json"), json!({"machines": []}));
    }

    #[test]
    fn reads_as_a_line_per_machine_with_its_command_aligned() {
        let known = [
            machine("devbox", None),
            machine("me@10.0.0.5", Some("~/bin/cornercase")),
            machine("gpu", Some("/opt/cc")),
        ];
        assert_eq!(
            known_text(&known),
            "devbox\nme@10.0.0.5  --command ~/bin/cornercase\ngpu          --command /opt/cc\n"
        );
    }

    #[test]
    fn says_when_no_machine_was_reached() {
        assert_eq!(known_text(&[]), format!("{NONE}\n"));
    }

    #[test]
    fn hidden_characters_are_escaped_in_the_text() {
        let known = [machine("devbox", Some("cc\u{1b}[2J"))];
        assert_eq!(known_text(&known), "devbox  --command cc\\u{1b}[2J\n");
    }
}
