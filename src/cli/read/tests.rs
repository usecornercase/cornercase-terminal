use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::{Cli, Command, Control};

fn read(args: &[&str]) -> ReadArgs {
    let cli = Cli::try_parse_from(["cornercase", "read"].into_iter().chain(args.iter().copied())).expect("parse");
    let Some(Command::Control(Control::Read(read))) = cli.command else { panic!("not read") };
    read
}

#[test]
fn panes_come_before_tabs_and_repeated_ids_are_read_once() {
    let read = read(&["--last-message", "--tab", "9", "--pane", "5", "--pane", "4", "--pane", "5", "--tab", "9"]);

    assert_eq!(
        targets(&read).expect("targets"),
        [
            LastMessage { pane: Some(5), tab: None },
            LastMessage { pane: Some(4), tab: None },
            LastMessage { pane: None, tab: Some(9) },
        ]
    );
}

#[rstest]
#[case::panes(&["--pane", "4", "--pane", "5"])]
#[case::tabs(&["--tab", "4", "--tab", "5"])]
#[case::both(&["--pane", "4", "--tab", "5"])]
#[case::repeated(&["--pane", "4", "--pane", "4"])]
fn several_screens_are_wrong_usage(#[case] args: &[&str]) {
    assert!(matches!(targets(&read(args)), Err(Error::WrongUsage(_))));
}

#[test]
fn messages_and_errors_have_their_own_blocks_with_unknown_metadata_left_clear() {
    let panes = [
        json!({ "pane": 5, "agent": "codex", "turn_over": true, "written": "2026-10-10T12:00:00.000Z", "text": "PR opened.\nCI passed." }),
        json!({ "pane": 6, "error": "no agent" }),
        json!({ "pane": 7, "agent": "claude", "turn_over": false, "text": "Still working." }),
        json!({ "tab": 8, "error": "no tab" }),
        json!({ "pane": 9, "agent": "opencode", "text": "Status unknown." }),
    ];

    assert_eq!(
        blocks(&panes),
        "pane 5  codex  turn_over=true  written=2026-10-10T12:00:00.000Z\nPR opened.\nCI passed.\n\n\
             pane 6\nerror: no agent\n\n\
             pane 7  claude  turn_over=false  written=unknown\nStill working.\n\n\
             tab 8\nerror: no tab\n\n\
             pane 9  opencode  turn_over=unknown  written=unknown\nStatus unknown.\n"
    );
}

mod a_target_that_fails {
    use super::*;

    #[rstest]
    #[case::pane(LastMessage { pane: Some(6), tab: None }, json!({ "pane": 6, "error": "no agent" }))]
    #[case::tab(LastMessage { pane: None, tab: Some(8) }, json!({ "tab": 8, "error": "no agent" }))]
    fn a_refused_target_keeps_its_error_and_its_id(#[case] target: LastMessage, #[case] expected: Value) {
        let kept = kept(&target, Err(Error::Control("no agent".into())));

        assert_eq!(kept.expect("kept"), expected);
    }

    #[rstest]
    #[case::no_server(Error::NoServer)]
    #[case::gone(Error::ServerGone)]
    #[case::old(Error::OldServer("read --last-message"))]
    fn a_server_that_cannot_be_reached_fails_the_whole_read(#[case] error: Error) {
        let target = LastMessage { pane: Some(6), tab: None };

        let kept = kept(&target, Err(error));

        assert!(matches!(kept, Err(Error::NoServer | Error::ServerGone | Error::OldServer(_))));
    }
}
