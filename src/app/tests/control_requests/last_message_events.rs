#![cfg(unix)]

use super::*;
use crate::agents::Known;
use crate::test_util::fixture;

fn thread() -> Vec<&'static str> {
    fixture!("claude/2.1.332/thread.jsonl").lines().collect()
}

fn last_messages(streamed: &[Streamed], id: u64) -> usize {
    told(streamed)
        .into_iter()
        .filter(|(what, ids)| {
            matches!(what, wire::What::LastMessage { agent } if agent == "claude") && ids.pane == Some(id)
        })
        .count()
}

fn until_settled(agent: &mut Agent, id: u64) -> Vec<Streamed> {
    let record = pane(&agent.app, id).context.record_for(Known::Claude).expect("a record");
    let Agent { app, rx, .. } = agent;
    let mut got = Vec::new();
    wait_until("the last message is read", || {
        drain(app, rx);
        app.refresh(Instant::now());
        got.extend(streamed(app));
        app.clients.events.last_messages.settled(id, &record.stamp())
    });
    got
}

fn read_step(agent: &mut Agent) {
    let read = agent.app.agents.watched();
    agent.refreshing("a read step", |a| a.agents.watched() != read);
}

fn following(only_its_pane: bool) -> (Agent, u64) {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.transcript(id, &thread()[..1]);
    let panes = if only_its_pane { vec![id] } else { Vec::new() };
    follow(&mut agent.app, &panes);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 0);
    (agent, id)
}

#[test]
fn an_answer_that_grows_is_told_once_per_step() {
    let (mut agent, id) = following(false);
    let lines = thread();

    agent.transcript(id, &[lines[2]]);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
    agent.transcript(id, &[lines[6]]);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
    let more = lines[9].replace("Both are done, with a test.", "And the docs.");
    agent.transcript(id, &[lines[9], &more]);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
}

#[test]
fn nothing_is_told_while_the_message_stays() {
    let (mut agent, id) = following(false);
    let lines = thread();
    agent.transcript(id, &[lines[2]]);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);

    agent.transcript(id, &[lines[3], lines[4], lines[5]]);

    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 0);
}

#[test]
fn a_turn_of_the_fake_is_told() {
    let mut agent = Agent::new();
    agent.fake.signal("thread");
    let id = agent.start(None);
    agent.transcript(id, &[]);
    follow(&mut agent.app, &[]);
    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 0);

    ask(&mut agent.app, None, send_text(id, "fix it", true, false));
    done(agent.answered("the enter is pressed"));
    turn(&mut agent, id);

    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
}

#[test]
fn readers_of_other_panes_are_not_told_and_nothing_is_read_for_them() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.transcript(id, &thread()[..1]);
    let shell = done(now(&mut agent.app, None, new_tab(None))).ids.pane.expect("the shell's pane");
    follow(&mut agent.app, &[shell]);

    agent.transcript(id, &[thread()[2]]);
    read_step(&mut agent);
    read_step(&mut agent);

    assert_eq!(last_messages(&streamed(&mut agent.app), id), 0);
    assert!(agent.app.clients.events.last_messages.is_empty());
}

#[test]
fn a_followed_pane_is_told() {
    let (mut agent, id) = following(true);

    agent.transcript(id, &[thread()[2]]);

    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
}

#[test]
fn nothing_is_read_while_nobody_follows() {
    let mut agent = Agent::new();
    let id = agent.start(None);
    agent.transcript(id, &thread()[..3]);
    let read = agent.app.agents.watched();
    agent.refreshing("two reads", |a| a.agents.watched() != read);

    assert!(agent.app.clients.events.last_messages.is_empty());
    follow(&mut agent.app, &[]);
    until_settled(&mut agent, id);
    agent.app.clients.events.forget(CLIENT);
    assert!(agent.app.clients.events.last_messages.is_empty());
}

#[test]
fn an_agent_found_while_following_is_told_its_first_message() {
    let mut agent = Agent::new();
    follow(&mut agent.app, &[]);
    read_step(&mut agent);
    let id = agent.start(None);
    let lines = thread();
    agent.transcript(id, &[lines[0]]);

    agent.transcript(id, &[lines[2]]);

    assert_eq!(last_messages(&until_settled(&mut agent, id), id), 1);
}
