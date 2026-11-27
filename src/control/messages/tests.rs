use rstest::rstest;

use super::*;
use crate::agents::context::Role;
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn told(id: u64, role: Role, text: &str, written: Option<&str>) -> Told {
    Told { id, role, text: text.into(), written: written.map(str::to_string) }
}

fn written(thread: &Thread) -> String {
    serde_json::to_string_pretty(thread).expect("json")
}

#[test]
fn a_page_is_pinned_by_its_fixture() {
    let page = Page {
        conversation: Some("3f2a9c1e-4b7d-4e2a-9f10-6c8d2b4a1e37".into()),
        messages: vec![
            told(41, Role::Prompt, "Add the login page", Some("2026-11-10T09:12:03Z")),
            told(42, Role::Reply, "Done. The page is in src/login.rs.", None),
        ],
        more_before: true,
    };

    let thread = Thread::of(7, "claude".into(), page);

    assert_eq!(written(&thread), fixture!("control/read-messages/v1/page.json").trim_end());
}

#[test]
fn an_empty_page_without_a_conversation_is_pinned_by_its_fixture() {
    let thread = Thread::of(3, "opencode".into(), Page::default());

    assert_eq!(written(&thread), fixture!("control/read-messages/v1/empty.json").trim_end());
}

#[rstest]
#[case::page(fixture!("control/read-messages/v1/page.json"))]
#[case::empty(fixture!("control/read-messages/v1/empty.json"))]
fn each_fixture_is_read_and_written_back_the_same(#[case] fixture: &str) {
    let read: Thread = serde_json::from_str(fixture).expect("an answer");

    assert_eq!(written(&read), fixture.trim_end());
}

#[test]
fn the_request_names_its_pane_and_its_slice() {
    let request: Request =
        serde_json::from_str(r#"{"command": "read-messages", "args": {"pane": 7, "last": 5, "before": 41}}"#)
            .expect("a request");

    let read = ReadMessages { pane: Some(7), tab: None, last: Some(5), before: Some(41), after: None };
    assert_eq!(request.command, Command::ReadMessages(read));
    let sent = serde_json::to_value(&request.command).expect("json");
    assert_eq!(
        sent,
        serde_json::json!({
            "command": "read-messages",
            "args": { "pane": 7, "tab": null, "last": 5, "before": 41, "after": null }
        })
    );
}
