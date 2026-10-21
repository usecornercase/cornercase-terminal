use serde_json::{Value, json};

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn pane(pane: u64, tab: u64, role: Option<&str>, resumes: bool) -> ClosedPane {
    ClosedPane {
        pane,
        tab,
        agent: Some("claude".into()),
        role: role.map(str::to_string),
        status: Some("working".into()),
        resumes,
    }
}

fn closed() -> AgentsClosed {
    AgentsClosed {
        project: 1,
        workspace: 2,
        tabs: vec![5, 6],
        agents: vec![pane(31, 5, Some("coder"), true), pane(32, 6, None, false)],
        stops: vec![PaneRef { pane: 33, agent: None, project: 1, workspace: 2, tab: 5 }],
        run: Some(9),
        closed: true,
        closed_agents: 2,
    }
}

fn reopened() -> AgentsReopened {
    AgentsReopened {
        project: 1,
        workspace: 2,
        tabs: vec![7, 8],
        panes: vec![pane(41, 7, Some("coder"), true), pane(42, 8, None, false)],
        run: Some(9),
    }
}

#[test]
fn every_documented_request_reads_with_no_stray_field() {
    let requests: Vec<Value> =
        serde_json::from_str(fixture!("control/closed-agents/v1/requests.json")).expect("requests");

    for raw in requests {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");
        assert_eq!(unknown_fields(&raw, &parsed), None, "{raw}");
    }
}

#[test]
fn the_requests_write_every_field() {
    let close = Command::AgentsClose(AgentsClose::default());
    let reopen = Command::AgentsReopen(AgentsReopen::default());

    assert_eq!(
        serde_json::to_value(&close).expect("json")["args"],
        json!({"workspace": null, "check": false, "force": false})
    );
    assert_eq!(serde_json::to_value(&reopen).expect("json")["args"], json!({"workspace": null}));
}

#[test]
fn the_requests_log_their_fields_and_no_text() {
    let close = Command::AgentsClose(AgentsClose { workspace: Some(2), check: true, force: false });

    assert_eq!(close.name(), "agents-close");
    assert!(close.fields().contains(&("workspace", "2".into())));
    assert!(close.fields().contains(&("check", "true".into())));
    assert_eq!(Command::AgentsReopen(AgentsReopen::default()).name(), "agents-reopen");
    assert_eq!(Command::AgentsForget(AgentsForget::default()).name(), "agents-forget");
}

#[test]
fn the_closed_answer_is_written_as_its_fixture_and_reads_back() {
    let written = serde_json::to_string_pretty(&closed()).expect("json");
    let read: AgentsClosed = serde_json::from_str(fixture!("control/closed-agents/v1/closed.json")).expect("answer");

    assert_eq!(written, fixture!("control/closed-agents/v1/closed.json").trim_end());
    assert_eq!(read, closed());
}

#[test]
fn the_reopened_answer_is_written_as_its_fixture_and_reads_back() {
    let written = serde_json::to_string_pretty(&reopened()).expect("json");
    let read: AgentsReopened =
        serde_json::from_str(fixture!("control/closed-agents/v1/reopened.json")).expect("answer");

    assert_eq!(written, fixture!("control/closed-agents/v1/reopened.json").trim_end());
    assert_eq!(read, reopened());
}

#[test]
fn an_answer_from_a_later_version_with_more_fields_still_reads() {
    let read: AgentsForgot =
        serde_json::from_value(json!({"project": 1, "workspace": 2, "forgot": 3, "run": null, "later": true}))
            .expect("answer");

    assert_eq!((read.workspace, read.forgot, read.run), (2, 3, None));
}

#[test]
fn the_contract_names_the_three_operations() {
    for name in ["agents-close", "agents-reopen", "agents-forget"] {
        assert!(Command::NAMES.contains(&name), "{name}");
    }
}
