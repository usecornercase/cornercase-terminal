#![cfg(unix)]

use super::*;
use crate::control::messages::ReadMessages;
use crate::test_util::fixture;

fn messages(pane: u64, last: Option<usize>, before: Option<u64>, after: Option<u64>) -> Command {
    Command::ReadMessages(ReadMessages { pane: Some(pane), tab: None, last, before, after })
}

fn read_later(agent: &mut Agent, command: Command) -> Value {
    ask(&mut agent.app, None, command);
    match agent.answered("the record is read") {
        Response::Ok(value) => value,
        Response::Error(message) => panic!("the read failed: {message}"),
    }
}

fn said(thread: &Value) -> Vec<(u64, String, String)> {
    let listed = thread["messages"].as_array().expect("messages");
    listed
        .iter()
        .map(|message| {
            let id = message["id"].as_u64().expect("an id");
            let role = message["role"].as_str().expect("a role").to_string();
            (id, role, message["text"].as_str().expect("a text").to_string())
        })
        .collect()
}

fn ids(thread: &Value) -> Vec<u64> {
    said(thread).into_iter().map(|(id, _, _)| id).collect()
}

fn with_the_fixture() -> (Agent, u64) {
    let mut agent = Agent::new();
    let id = agent.start(None);
    let lines: Vec<&str> = fixture!("claude/2.1.332/thread.jsonl").lines().collect();
    agent.transcript(id, &lines);
    agent.refreshing("the pane names its conversation", |a| pane(a, id).context.conversation().is_some());
    (agent, id)
}

fn turn(agent: &mut Agent, id: u64) {
    agent.refreshing("the agent works", |a| pane(a, id).agent.status() == Some(activity::Status::Working));
    agent.finish();
    agent.refreshing("the turn is over", |a| pane(a, id).agent.status() != Some(activity::Status::Working));
}

#[test]
fn a_conversation_is_read_in_order_with_ids() {
    let (mut agent, id) = with_the_fixture();

    let thread = read_later(&mut agent, messages(id, None, None, None));

    let conversation = pane(&agent.app, id).context.conversation().map(str::to_string);
    assert_eq!((thread["pane"].as_u64(), thread["agent"].as_str()), (Some(id), Some("claude")));
    assert_eq!(thread["conversation"].as_str().map(str::to_string), conversation);
    assert_eq!(
        said(&thread),
        [
            (1, "user".into(), "Add a login page".into()),
            (2, "assistant".into(), "I will read the routes first.".into()),
            (3, "assistant".into(), "The page is in src/login.rs.".into()),
            (4, "user".into(), "Add a logout button too".into()),
            (5, "user".into(), "And a test for both".into()),
            (6, "assistant".into(), "Both are done, with a test.".into()),
        ]
    );
    assert_eq!(thread["messages"][0]["written"], "2026-11-27T10:00:00.000Z");
    assert_eq!(thread["more_before"], false);
}

#[test]
fn pages_follow_each_other() {
    let (mut agent, id) = with_the_fixture();

    let newest = read_later(&mut agent, messages(id, Some(2), None, None));
    let earlier = read_later(&mut agent, messages(id, Some(2), Some(5), None));
    let after = read_later(&mut agent, messages(id, None, None, Some(4)));

    assert_eq!((ids(&newest), &newest["more_before"]), (vec![5, 6], &json!(true)));
    assert_eq!((ids(&earlier), &earlier["more_before"]), (vec![3, 4], &json!(true)));
    assert_eq!((ids(&after), &after["more_before"]), (vec![5, 6], &json!(true)));
}

#[test]
fn what_is_typed_and_what_is_sent_both_appear() {
    let mut agent = Agent::new();
    agent.fake.signal("thread");
    let id = agent.start(None);
    ask(&mut agent.app, None, send_text(id, "from a program", true, false));
    done(agent.answered("the enter is pressed"));
    turn(&mut agent, id);
    assert!(agent.app.projects.pane_by_mut(id).expect("the pane").write(b"from the keyboard\r"));
    turn(&mut agent, id);

    let thread = read_later(&mut agent, messages(id, None, None, None));

    assert_eq!(
        said(&thread),
        [
            (1, "user".into(), "from a program".into()),
            (2, "assistant".into(), "done: from a program".into()),
            (3, "user".into(), "from the keyboard".into()),
            (4, "assistant".into(), "done: from the keyboard".into()),
        ]
    );
}

#[test]
fn a_shell_says_why_it_cannot_be_read() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, None, messages(id, None, None, None)));

    assert!(
        message.starts_with(&format!("pane {id} runs no agent, and cornercase only reads the messages of")),
        "{message}"
    );
}

#[test]
fn before_and_after_together_are_refused() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let message = error(now(&mut app, None, messages(id, None, Some(4), Some(2))));

    assert_eq!(message, "give --before or --after, not both");
}
