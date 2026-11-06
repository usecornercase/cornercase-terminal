use rstest::rstest;
use serde_json::json;

use super::*;
use crate::control::view::takes_view;
use crate::control::{Command, Request};

fn request(args: &serde_json::Value) -> serde_json::Result<Request> {
    serde_json::from_value(json!({"command": "held", "args": args}))
}

fn held(action: HeldAction) -> Command {
    Command::Held(HeldRequest { pane: Some(9), tab: None, action })
}

#[rstest]
#[case::send_now("send-now", HeldAction::SendNow)]
#[case::wait_turn("wait-turn", HeldAction::WaitTurn)]
fn a_request_names_its_button(#[case] name: &str, #[case] action: HeldAction) {
    let read = request(&json!({"pane": 9, "action": name})).expect("a request");

    assert_eq!(read.command, held(action));
    assert_eq!(read.command.name(), "held");
    assert_eq!(serde_json::to_value(&read.command).expect("json")["args"]["action"], name);
}

#[rstest]
#[case::no_button(json!({"pane": 9}))]
#[case::another_button(json!({"pane": 9, "action": "discard"}))]
fn a_request_needs_one_of_the_two_buttons(#[case] args: serde_json::Value) {
    assert!(request(&args).is_err(), "{args}");
}

#[test]
fn a_request_may_leave_its_pane_to_the_defaults_of_send() {
    let read = request(&json!({"action": "wait-turn"})).expect("a request");

    assert_eq!(read.command, Command::Held(HeldRequest { pane: None, tab: None, action: HeldAction::WaitTurn }));
}

#[test]
fn the_log_says_the_pane_and_the_button() {
    assert_eq!(held(HeldAction::SendNow).fields(), [("pane", "9".to_string()), ("action", "send-now".to_string())]);
}

#[test]
fn it_takes_a_view_as_send_does() {
    assert!(takes_view(&held(HeldAction::WaitTurn)));
}

#[test]
fn an_answer_always_has_its_four_keys() {
    let nothing = serde_json::to_value(HeldInfo { pane: 9, ..HeldInfo::default() }).expect("json");
    let from = HeldInfo { pane: 9, held_sends: 2, held_from: Some(40), held_queued: true };

    assert_eq!(nothing, json!({"pane": 9, "held_sends": 0, "held_from": null, "held_queued": false}));
    assert_eq!(
        serde_json::to_value(from).expect("json"),
        json!({"pane": 9, "held_sends": 2, "held_from": 40, "held_queued": true})
    );
}

#[test]
fn an_answer_with_fields_this_command_does_not_know_still_reads() {
    let read: HeldInfo = serde_json::from_value(json!({"pane": 9, "held_sends": 1, "later": true})).expect("an answer");

    assert_eq!(read, HeldInfo { pane: 9, held_sends: 1, ..HeldInfo::default() });
}
