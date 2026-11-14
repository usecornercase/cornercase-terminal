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
