#![cfg(unix)]

use super::*;
use crate::control::pending::PendingQuestion;
use crate::test_util::fixture;

fn pending(pane: u64) -> Command {
    Command::PendingQuestion(PendingQuestion { pane: Some(pane), tab: None })
}

fn read_later(agent: &mut Agent, id: u64) -> Value {
    ask(&mut agent.app, None, pending(id));
    match agent.answered("the record is read") {
        Response::Ok(value) => value,
        Response::Error(message) => panic!("the read failed: {message}"),
    }
}

fn waiting(agent: &mut Agent, id: u64) {
    agent.refreshing("the agent waits and knows its conversation", |a| {
        let term = pane(a, id);
        term.agent.status() == Some(activity::Status::Waiting)
            && term.context.record_for(crate::agents::Known::Claude).is_some()
    });
}

const BASH: &str = r#"{"type":"assistant","isSidechain":false,"message":{"id":"b1","role":"assistant","content":[{"type":"tool_use","id":"tb1","name":"Bash","input":{"command":"cargo test --locked"}}]}}"#;

#[test]
fn a_question_is_read_from_the_record_while_the_agent_waits_for_it() {
    let mut agent = Agent::new();
    let id = agent.start(Some("please choose"));
    agent.finish();
    waiting(&mut agent, id);

    let asked = read_later(&mut agent, id);

    let mut expected: Value =
        serde_json::from_str(fixture!("control/pending-question/v1/question.json")).expect("json");
    expected["pane"] = id.into();
    assert_eq!(asked, expected);
}

#[test]
fn an_answered_question_is_no_longer_pending() {
    let mut agent = Agent::new();
    let id = agent.start(Some("please choose"));
    agent.finish();
    waiting(&mut agent, id);

    done(now(&mut agent.app, None, press_keys(id, &["1", "enter"])));
    agent.refreshing("the agent goes on", |a| pane(a, id).agent.status() != Some(activity::Status::Waiting));

    assert_eq!(value(&mut agent.app, None, pending(id)), Value::Null);
}

#[test]
fn a_permission_names_its_tool_and_command() {
    let mut agent = Agent::new();
    let id = agent.start(Some("please ask"));
    agent.finish();
    waiting(&mut agent, id);
    agent.transcript(id, &[BASH]);

    let asked = read_later(&mut agent, id);

    let mut expected: Value =
        serde_json::from_str(fixture!("control/pending-question/v1/permission.json")).expect("json");
    expected["pane"] = id.into();
    assert_eq!(asked, expected);
}

#[test]
fn a_tool_that_runs_while_the_agent_works_is_not_pending() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    ask(&mut agent.app, None, send_text(id, "next step", true, false));
    done(agent.answered("the enter is pressed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.transcript(id, &[BASH]);

    assert_eq!(value(&mut agent.app, None, pending(id)), Value::Null);
}

#[test]
fn a_wait_the_record_does_not_say_is_nothing_pending() {
    let mut agent = Agent::new();
    let id = agent.start(Some("please ask"));
    agent.finish();
    waiting(&mut agent, id);

    assert_eq!(read_later(&mut agent, id), Value::Null);
}

#[test]
fn a_shell_says_why_it_cannot_be_read() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, None, pending(id)));

    assert!(
        message.starts_with(&format!(
            "pane {id} runs no agent, and cornercase reads what an agent asks only from Claude Code's records"
        )),
        "{message}"
    );
}
