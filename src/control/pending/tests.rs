use rstest::rstest;

use super::*;
use crate::agents::context::{Choice, Question};
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn asking(awaited: Awaited) -> Asking {
    Asking { pane: 7, agent: "claude".into(), awaited }
}

fn choice(number: usize, label: &str, description: &str) -> Choice {
    Choice { number, label: label.into(), description: Some(description.into()), selected: None }
}

fn written(asking: &Asking) -> String {
    serde_json::to_string_pretty(asking).expect("json")
}

#[test]
fn a_question_is_pinned_by_its_fixture() {
    let question = Question {
        header: Some("Database".into()),
        text: "Which database should the service use?".into(),
        multi_select: false,
        free_text: true,
        options: vec![choice(1, "Postgres", "What the other services use"), choice(2, "SQLite", "One file, no server")],
    };

    let asking = asking(Awaited::questions(vec![question]));

    assert_eq!(written(&asking), fixture!("control/pending-question/v1/question.json").trim_end());
}

#[test]
fn a_permission_is_pinned_by_its_fixture() {
    let asking = asking(Awaited::permission("Bash".into(), Some("cargo test --locked".into())));

    assert_eq!(written(&asking), fixture!("control/pending-question/v1/permission.json").trim_end());
}

#[rstest]
#[case::question(fixture!("control/pending-question/v1/question.json"))]
#[case::questions(fixture!("control/pending-question/v1/questions.json"))]
#[case::multi_select(fixture!("control/pending-question/v1/multi-select.json"))]
#[case::permission(fixture!("control/pending-question/v1/permission.json"))]
fn each_fixture_is_read_and_written_back_the_same(#[case] fixture: &str) {
    let read: Asking = serde_json::from_str(fixture).expect("an answer");

    assert_eq!(written(&read), fixture.trim_end());
}

#[test]
fn the_request_names_its_pane() {
    let request: Request =
        serde_json::from_str(r#"{"command": "pending-question", "args": {"pane": 7}}"#).expect("a request");

    assert_eq!(request.command, Command::PendingQuestion(PendingQuestion { pane: Some(7), tab: None }));
    let sent = serde_json::to_value(&request.command).expect("json");
    assert_eq!(sent, serde_json::json!({ "command": "pending-question", "args": { "pane": 7, "tab": null } }));
}
