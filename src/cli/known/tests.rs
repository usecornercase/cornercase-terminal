use clap::error::ErrorKind;
use rstest::rstest;
use serde_json::json;

use super::*;
use crate::cli::args::{Command as Run, RemoteArgs};
use crate::cli::tests::parse;
use crate::machines::list::Here;
use crate::remote::machines::NOT_A_DESTINATION;
use crate::test_util::{TempDir, unwritable_file_in};

fn machine(name: &str, destination: &str, command: Option<&str>) -> Machine {
    Machine { command: command.map(Into::into), ..machines::machine(name, destination) }
}

fn not_reached(name: &str, destination: &str, command: Option<&str>) -> Machine {
    Machine { unreached: true, ..machine(name, destination, command) }
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
    fn show_every_form_in_the_usage() {
        let text = parse(&["remote"]).expect_err("no destination").to_string();
        for form in [
            "cornercase remote [OPTIONS] <DESTINATION>",
            "cornercase remote --known [--json]",
            "cornercase remote --remember <DESTINATION> [--command <COMMAND>]",
            "cornercase remote --forget <DESTINATION>",
        ] {
            assert!(text.contains(form), "{text}");
        }
    }

    #[test]
    fn remember_and_forget_need_no_window() {
        let adding = remote(&["remote", "--remember", "devbox", "--command", "cc"]);
        assert_eq!(
            (adding.remember.as_deref(), adding.command.as_deref(), adding.destination, adding.known),
            (Some("devbox"), Some("cc"), None, false)
        );
        let taking = remote(&["remote", "--forget", "devbox"]);
        assert_eq!((taking.forget.as_deref(), taking.remember), (Some("devbox"), None));
    }

    #[rstest]
    #[case::both(&["remote", "--remember", "a", "--forget", "b"])]
    #[case::a_window(&["remote", "devbox", "--remember", "x"])]
    #[case::the_list(&["remote", "--known", "--remember", "x"])]
    #[case::the_list_forgetting(&["remote", "--known", "--forget", "x"])]
    #[case::a_command_to_forget(&["remote", "--forget", "x", "--command", "cc"])]
    #[case::json_remembering(&["remote", "--remember", "x", "--json"])]
    #[case::json_forgetting(&["remote", "--forget", "x", "--json"])]
    fn refuse_what_means_nothing_for_them(#[case] args: &[&str]) {
        let error = parse(args).expect_err("conflict");
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }

    #[rstest]
    #[case::a_space("--remember=dev box")]
    #[case::an_option("--remember=-oProxyCommand=sh")]
    #[case::a_tab("--forget=dev\tbox")]
    #[case::too_long(&format!("--remember={}", "a".repeat(256)))]
    fn a_destination_ssh_cannot_take_is_wrong_usage(#[case] arg: &str) {
        let error = parse(&["remote", arg]).expect_err("refused");
        assert_eq!((error.exit_code(), error.kind()), (2, ErrorKind::ValueValidation));
        assert!(error.to_string().contains(NOT_A_DESTINATION), "{error}");
    }

    #[test]
    fn an_option_as_the_next_word_is_wrong_usage_too() {
        assert_eq!(parse(&["remote", "--remember", "-oProxyCommand=sh"]).expect_err("refused").exit_code(), 2);
    }
}

mod the_list {
    use super::*;

    #[test]
    fn prints_each_machine_with_its_name_and_command_or_null() {
        let known = [machine("devbox", "devbox", None), machine("ten", "me@10.0.0.5", Some("~/bin/cornercase"))];
        assert_eq!(
            serde_json::to_value(Known::from(&known[..])).expect("json"),
            json!({"machines": [
                {"name": "devbox", "destination": "devbox", "command": null},
                {"name": "ten", "destination": "me@10.0.0.5", "command": "~/bin/cornercase"},
            ]})
        );
    }

    #[test]
    fn a_machine_no_window_reached_says_so() {
        let known = [machine("devbox", "devbox", None), not_reached("newbox", "newbox", None)];
        assert_eq!(
            serde_json::to_value(Known::from(&known[..])).expect("json"),
            json!({"machines": [
                {"name": "devbox", "destination": "devbox", "command": null},
                {"name": "newbox", "destination": "newbox", "command": null, "unreached": true},
            ]})
        );
        let known = [not_reached("newbox", "newbox", None), not_reached("gpu", "gpu", Some("/opt/cc"))];
        assert_eq!(
            known_text(&known),
            "newbox  newbox  (not reached yet)\ngpu     gpu     --command /opt/cc  (not reached yet)\n"
        );
    }

    #[test]
    fn an_empty_list_is_an_empty_array() {
        assert_eq!(serde_json::to_value(Known::from(&[][..])).expect("json"), json!({"machines": []}));
    }

    #[test]
    fn reads_as_a_line_per_machine_with_its_command_aligned() {
        let known = [
            machine("a", "devbox", None),
            machine("bb", "me@10.0.0.5", Some("~/bin/cornercase")),
            machine("c", "gpu", Some("/opt/cc")),
        ];
        assert_eq!(
            known_text(&known),
            "a   devbox\nbb  me@10.0.0.5  --command ~/bin/cornercase\nc   gpu          --command /opt/cc\n"
        );
    }

    #[test]
    fn says_when_the_list_is_empty() {
        assert_eq!(known_text(&[]), format!("{NONE}\n"));
    }

    #[test]
    fn hidden_characters_are_escaped_in_the_text() {
        let known = [machine("devbox", "devbox", Some("cc\u{1b}[2J"))];
        assert_eq!(known_text(&known), "devbox  devbox  --command cc\\u{1b}[2J\n");
    }
}

mod writing {
    use super::*;

    fn here_in(dir: &TempDir) -> Here {
        Here { config: dir.path().join("config.json"), legacy: dir.path().join("machines.json") }
    }

    fn not_listed(result: Result<String>) -> String {
        let Err(Error::Control(message)) = result else { panic!("an error, got {result:?}") };
        message
    }

    #[test]
    fn remembering_a_new_machine_says_a_window_asks() {
        let dir = TempDir::new();
        let here = here_in(&dir);

        let said = remembered(&here, "newbox".into(), None).expect("written");

        assert_eq!(said, "remembered newbox; a window asks before it first connects there\n");
        assert_eq!(list::read(&here).iter().map(|machine| machine.unreached).collect::<Vec<_>>(), [true]);
    }

    #[test]
    fn remembering_a_machine_a_window_reached_says_only_so() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(
            &here.config,
            r#"{"machines":[{"name":"devbox","icon":"☁","destination":"devbox","folder":"~"}]}"#,
        )
        .expect("a list");

        let said = remembered(&here, "devbox".into(), Some("cc".into())).expect("written");

        assert_eq!(said, "remembered devbox\n");
        assert_eq!(list::read(&here)[0].command.as_deref(), Some("cc"));
    }

    #[test]
    fn forgetting_a_listed_machine_says_so() {
        let dir = TempDir::new();
        let here = here_in(&dir);
        remembered(&here, "devbox".into(), None).expect("written");

        assert_eq!(forgotten(&here, "devbox").expect("written"), "forgot devbox\n");
        assert_eq!(list::read(&here), []);
    }

    #[test]
    fn forgetting_a_machine_not_listed_fails_and_writes_nothing() {
        let dir = TempDir::new();
        let here = here_in(&dir);

        assert!(not_listed(forgotten(&here, "newbox")).contains("is not on this computer's list"));
        assert!(!here.config.exists());
    }

    #[rstest]
    #[case::broken_json("{")]
    #[case::not_an_object("\"text\"")]
    fn a_config_it_cannot_read_is_left_as_it_is(#[case] bytes: &str) {
        let dir = TempDir::new();
        let here = here_in(&dir);
        std::fs::write(&here.config, bytes).expect("a broken config");

        assert!(not_listed(remembered(&here, "newbox".into(), None)).starts_with("cannot read or write"));
        assert!(not_listed(forgotten(&here, "devbox")).starts_with("cannot read or write"));
        assert_eq!(std::fs::read_to_string(&here.config).expect("the config"), bytes);
    }

    #[test]
    fn a_config_it_cannot_write_says_where() {
        let dir = TempDir::new();
        let here = Here { config: unwritable_file_in(&dir, "config.json"), legacy: dir.path().join("machines.json") };

        assert!(not_listed(remembered(&here, "devbox".into(), None)).starts_with("cannot read or write"));
    }

    #[test]
    fn a_refused_command_names_the_command_not_the_file() {
        let dir = TempDir::new();
        let here = Here { config: dir.path().join("config.json"), legacy: dir.path().join("machines.json") };

        let said = not_listed(remembered(&here, "devbox".into(), Some("a\tb".into())));

        assert!(said.contains("command") && !said.contains("config.json"), "{said}");
    }

    #[test]
    fn a_broken_old_file_is_named_and_the_config_is_not() {
        let dir = TempDir::new();
        let here = Here { config: dir.path().join("config.json"), legacy: dir.path().join("machines.json") };
        std::fs::write(&here.legacy, "[nope").expect("written");

        let said = not_listed(forgotten(&here, "devbox"));

        assert!(said.contains("machines.json") && !said.contains("cannot read or write"), "{said}");
    }

    #[test]
    fn a_held_lock_is_said_as_it_is_and_the_config_is_not_blamed() {
        let dir = TempDir::new();
        let here = Here { config: dir.path().join("config.json"), legacy: dir.path().join("machines.json") };
        let held = list::hold(&here.config).expect("held");

        let said = not_listed(forgotten(&here, "devbox"));

        assert!(said.contains("try again") && !said.contains("cannot read or write"), "{said}");
        drop(held);
    }
}
