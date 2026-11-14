use rstest::rstest;
use serde_json::json;

use super::*;
use crate::error::Error;

fn vps() -> Remote {
    Remote { destination: "vps".into(), command: None, option: "--remote-command" }
}

const NOT_CONFIRMED: &str = "not confirmed: the prompt may not have been submitted: the agent in pane 7 recorded no new prompt after the Enter; `cornercase read --pane 7` shows where it is, read it before sending again";

mod a_message {
    use super::*;

    #[test]
    fn names_the_destination_in_its_hint() {
        assert_eq!(
            vps().hinted(NOT_CONFIRMED),
            NOT_CONFIRMED.replace("`cornercase read", "`cornercase --remote vps read")
        );
    }

    #[test]
    fn names_the_remote_command_as_sh_would_read_it() {
        let remote = Remote { command: Some("~/bin/cornercase dev".into()), ..vps() };

        assert_eq!(
            remote.hinted("see `cornercase status`"),
            "see `cornercase --remote vps --remote-command '~/bin/cornercase dev' status`"
        );
    }

    #[test]
    fn names_it_in_every_hint() {
        assert_eq!(
            vps().hinted("`cornercase status` or `cornercase keys --pane 7 0`"),
            "`cornercase --remote vps status` or `cornercase --remote vps keys --pane 7 0`"
        );
    }

    #[test]
    fn is_the_same_when_rewritten_again() {
        let once = vps().hinted(NOT_CONFIRMED);

        assert_eq!(vps().hinted(&once), once);
    }

    #[rstest]
    #[case::already_remote("restart it with `cornercase --remote gpu restart`")]
    #[case::remote_after_the_command("run `cornercase read --pane 7 --remote gpu`")]
    #[case::the_window_alone("or open a window with `cornercase`")]
    #[case::not_a_command("see `held_sends`")]
    #[case::the_window("open one with `cornercase remote`")]
    #[case::the_skill("run `cornercase skill`")]
    #[case::the_server("run `cornercase server`")]
    #[case::the_proxy("run `cornercase proxy`")]
    #[case::window_processes("run `cornercase plugin windows` on its own")]
    #[case::for_the_machine_named("Run `cornercase update` there")]
    #[case::for_this_machine("Run `cornercase update` here")]
    #[case::unclosed("run `cornercase read --pane 7 shows it")]
    fn keeps_what_is_no_hint_for_that_server(#[case] text: &str) {
        assert_eq!(vps().hinted(text), text);
    }

    #[test]
    fn names_it_before_a_word_that_only_starts_like_there() {
        assert_eq!(
            vps().hinted("run `cornercase status` thereafter"),
            "run `cornercase --remote vps status` thereafter"
        );
    }

    #[test]
    fn about_another_machines_cornercase_keeps_its_command() {
        let texts = [
            Error::RemoteTooOld("gpu".into()).to_string(),
            Error::RemoteOlder { host: "gpu".into(), there: "0.1.0".into() }.to_string(),
            Error::RemoteNewer { host: "gpu".into(), there: "99.0.0".into() }.to_string(),
        ];

        for text in texts {
            assert_eq!(vps().hinted(&text), text);
        }
    }
}

mod an_answer {
    use super::*;

    fn hinted(mut value: Value) -> Value {
        let text = value.to_string();
        vps().hint_answer(&text, &mut value);
        value
    }

    #[test]
    fn names_the_destination_in_every_error_at_any_depth() {
        let answer = json!({"error": "see `cornercase status`", "panes": [{"error": null}, {"error": "see `cornercase status`"}]});

        assert_eq!(
            hinted(answer),
            json!({
                "error": "see `cornercase --remote vps status`",
                "panes": [{"error": null}, {"error": "see `cornercase --remote vps status`"}]
            })
        );
    }

    #[test]
    fn keeps_every_other_field_as_it_is() {
        let answer = json!({
            "text": "`cornercase read --pane 3`",
            "why": "`cornercase read --pane 3`",
            "note": "`cornercase read --pane 3`",
            "error": {"detail": "`cornercase status`", "error": "`cornercase status`"}
        });

        assert_eq!(
            hinted(answer),
            json!({
                "text": "`cornercase read --pane 3`",
                "why": "`cornercase read --pane 3`",
                "note": "`cornercase read --pane 3`",
                "error": {"detail": "`cornercase status`", "error": "`cornercase --remote vps status`"}
            })
        );
    }
}

mod a_relayed_message {
    use clap::CommandFactory;

    use super::*;
    use crate::cli::Cli;

    const NO_AGENT: &str = "say which agent to start, such as `cornercase start claude`: settings → agents is on auto and no agent runs here";

    fn on_vps(text: &str) -> String {
        on_machine("vps", text)
    }

    #[rstest]
    #[case::start(
        NO_AGENT,
        "say which agent to start, such as `cornercase start --machine vps claude`: settings → agents is on auto and no agent runs here"
    )]
    #[case::flow_run("`cornercase flow run Quick --prompt x`", "`cornercase flow run --machine vps Quick --prompt x`")]
    #[case::new_workspace("`cornercase new-workspace notes`", "`cornercase new-workspace --machine vps notes`")]
    #[case::focus("`cornercase focus --workspace 4`", "`cornercase focus --machine vps --workspace 4`")]
    #[case::stats("`cornercase stats`", "`cornercase stats --machine vps`")]
    #[case::usage("`cornercase usage`", "`cornercase usage --machine vps`")]
    fn names_the_machine_in_a_command_that_takes_it(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(on_vps(text), expected);
    }

    #[rstest]
    #[case::flow_list("(`cornercase flow list`)", "(`cornercase flow list` there)")]
    #[case::profile("`cornercase profile list` lists them", "`cornercase profile list` there lists them")]
    #[case::status_panes("`cornercase status --panes`", "`cornercase status --panes` there")]
    #[case::focus_with_a_pane("`cornercase focus --pane 3`", "`cornercase focus --pane 3` there")]
    fn says_there_after_a_command_without_it(#[case] text: &str, #[case] expected: &str) {
        assert_eq!(on_vps(text), expected);
    }

    #[rstest]
    #[case::status("read `cornercase status` again")]
    #[case::status_json("read `cornercase status --json` again")]
    #[case::machine_after("`cornercase start claude --machine gpu`")]
    #[case::machine_equals("`cornercase start --machine=gpu claude`")]
    #[case::remote("`cornercase --remote gpu start claude`")]
    #[case::there("`cornercase flow list` there")]
    #[case::here("`cornercase flow list` here")]
    #[case::the_window("or open a window with `cornercase`")]
    #[case::unclosed("run `cornercase flow list shows it")]
    fn keeps_what_names_the_machine_already_or_is_no_hint(#[case] text: &str) {
        assert_eq!(on_vps(text), text);
    }

    #[rstest]
    #[case(NO_AGENT)]
    #[case("(`cornercase flow list`) and `cornercase status --panes`")]
    #[case("`cornercase focus --pane 3`")]
    fn is_the_same_when_rewritten_again(#[case] text: &str) {
        let once = on_vps(text);

        assert_eq!(on_vps(&once), once);
    }

    #[test]
    fn names_the_machine_in_every_error_at_any_depth_and_nowhere_else() {
        let mut answer = json!({
            "text": "`cornercase flow list`",
            "why": "`cornercase flow list`",
            "error": "see `cornercase flow list`",
            "panes": [{"error": "`cornercase stats`"}],
            "note": {"error": {"error": "`cornercase usage`"}}
        });

        errors_on_machine("vps", &mut answer);

        assert_eq!(
            answer,
            json!({
                "text": "`cornercase flow list`",
                "why": "`cornercase flow list`",
                "error": "see `cornercase flow list` there",
                "panes": [{"error": "`cornercase stats --machine vps`"}],
                "note": {"error": {"error": "`cornercase usage --machine vps`"}}
            })
        );
    }

    fn walk(command: &clap::Command, prefix: &str, checked: &mut Vec<String>) {
        for sub in command.get_subcommands() {
            let path = format!("{prefix}{}", sub.get_name());
            let takes = sub.get_arguments().any(|arg| arg.get_id() == "machine");
            let named = on_vps(&format!("`cornercase {path}`")).contains("--machine vps");
            assert_eq!(named, takes, "`cornercase {path}`");
            checked.push(path.clone());
            walk(sub, &format!("{path} "), checked);
        }
    }

    #[test]
    fn every_command_that_takes_machine_gets_it_and_no_other_does() {
        let mut checked = Vec::new();
        walk(&Cli::command(), "", &mut checked);

        for (path, _) in ON_A_MACHINE {
            assert!(checked.iter().any(|name| name == path), "{path}");
        }
    }

    #[test]
    fn focus_is_given_no_machine_next_to_what_conflicts_with_it() {
        let cli = Cli::command();
        let focus = cli.find_subcommand("focus").expect("focus");
        let machine = focus.get_arguments().find(|arg| arg.get_id() == "machine").expect("--machine");
        let mut conflicts: Vec<String> = focus
            .get_arg_conflicts_with(machine)
            .iter()
            .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
            .collect();
        conflicts.sort();
        let (_, refused) = ON_A_MACHINE.iter().find(|(path, _)| *path == "focus").expect("focus");
        let mut refused: Vec<String> = refused.iter().map(ToString::to_string).collect();
        refused.sort();

        assert_eq!(refused, conflicts);
    }

    #[rstest]
    #[case::start(NO_AGENT)]
    #[case::flow_run("`cornercase flow run Quick --prompt x`")]
    #[case::status("read `cornercase status` again")]
    #[case::focus("`cornercase focus --workspace 4`")]
    fn with_remote_it_names_both_in_an_order_the_cli_reads(#[case] text: &str) {
        let remote = Remote { destination: "devbox".into(), command: None, option: "--remote-command" };
        let said = remote.hinted(&on_vps(text));

        for span in said.split('`').skip(1).step_by(2).filter(|span| span.starts_with("cornercase ")) {
            let words = span.split(' ');
            let parsed = Cli::command().try_get_matches_from(words);
            assert!(parsed.is_ok(), "{span}: {:?}", parsed.err());
            assert!(span.contains("--remote devbox"), "{span}");
            assert_eq!(span.contains("--machine vps"), !text.contains("status"), "{span}");
        }
    }

    #[test]
    fn with_remote_a_command_for_the_machine_is_left_alone() {
        let remote = Remote { destination: "devbox".into(), command: None, option: "--remote-command" };
        let text = on_vps("(`cornercase flow list`)");

        assert_eq!(remote.hinted(&text), text);
    }
}
