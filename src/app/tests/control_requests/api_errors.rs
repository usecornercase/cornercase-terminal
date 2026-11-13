#![cfg(unix)]

use super::agents::in_status;
use super::*;

const MESSAGE: &str = "Please run /login · API Error: 403";

fn failed_turn(agent: &mut Agent) -> u64 {
    let id = agent.start(None);
    ask(&mut agent.app, None, send_text(id, "api-error please", true, false));
    done(agent.answered("the prompt is confirmed"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.finish();
    agent.refreshing("the turn ends", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
    id
}

#[test]
fn a_turn_that_ended_on_an_api_error_is_failed_and_not_waiting() {
    let mut agent = Agent::new();
    let id = failed_turn(&mut agent);

    let info = in_status(&mut agent.app, id);

    assert_eq!(agent.status(id), Some(activity::Status::Failed));
    assert_eq!((info.status.as_deref(), info.dialog), (Some("failed"), false));
}

#[test]
fn a_wait_for_the_turn_over_says_it_failed_and_the_last_message_is_the_error() {
    let mut agent = Agent::new();
    let id = failed_turn(&mut agent);

    ask(&mut agent.app, None, wait_for(id, Until::TurnOver, Some(5.0)));
    let ended = done(agent.answered("the wait ends"));
    ask(&mut agent.app, None, Command::LastMessage(wire::LastMessage { pane: Some(id), tab: None }));
    let last = done(agent.answered("the transcript is read"));

    assert_eq!(ended.ended.as_deref(), Some("failed"));
    assert_eq!((last.turn_over, last.text.as_deref().map(|text| text.starts_with(MESSAGE))), (Some(true), Some(true)));
}

#[test]
fn send_takes_the_next_prompt_of_a_failed_turn_and_the_failure_goes() {
    let mut agent = Agent::new();
    let id = failed_turn(&mut agent);

    ask(&mut agent.app, None, send_text(id, "next", true, false));
    done(agent.answered("the next prompt is accepted"));
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.finish();
    agent.refreshing("the turn ends well", |a| pane(a, id).agent.status() != Some(activity::Status::Working));

    assert_ne!(agent.status(id), Some(activity::Status::Failed));
}
