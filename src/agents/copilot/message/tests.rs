use super::*;
use crate::agents::context::message::Role;
use crate::test_util::{TempDir, fixture};

const TURN: &str = fixture!("copilot/1.0.98/turn.jsonl");
const QUEUED: &str = fixture!("copilot/1.0.98/queued.jsonl");
const SUBAGENT: &str = fixture!("copilot/1.0.98/subagent.jsonl");
const CHUNKS: &str = fixture!("copilot/1.0.98/chunks.jsonl");
const NOT_PROMPTS: &str = fixture!("copilot/1.0.98/not-prompts.jsonl");

fn session(files: &[&str]) -> TempDir {
    let dir = TempDir::new();
    std::fs::write(dir.path().join("events.jsonl"), files.concat()).expect("write events.jsonl");
    dir
}

fn spoken(dir: &TempDir) -> Vec<(Role, String)> {
    let mut found = Vec::new();
    said_all(dir.path(), &mut |spoken| {
        found.push((spoken.role, spoken.said.text));
        true
    })
    .expect("read the session");
    found
}

#[test]
fn the_last_message_is_the_newest_main_reply_not_a_sub_agents() {
    let dir = session(&[TURN, SUBAGENT]);

    let said = last_message(dir.path()).expect("read").expect("a message");

    assert_eq!(said.text, "Hello from the fake model.");
}

#[test]
fn its_time_is_the_replys_timestamp() {
    let dir = session(&[TURN]);

    let said = last_message(dir.path()).expect("read").expect("a message");

    assert_eq!(said.written.as_deref(), Some("2026-10-17T17:23:03.300Z"));
}

#[test]
fn chunks_of_one_call_are_joined_in_order() {
    let dir = session(&[TURN, CHUNKS]);

    let said = last_message(dir.path()).expect("read").expect("a message");

    assert_eq!(said.text, "First half.\n\nSecond half.");
}

#[test]
fn an_empty_tool_step_is_not_a_message() {
    let empty = TURN.replace("Hello from the fake model.", "");
    let dir = session(&[&empty]);

    assert_eq!(last_message(dir.path()).expect("read"), None);
}

#[test]
fn everything_said_comes_newest_first_with_prompts_and_replies() {
    let dir = session(&[TURN, QUEUED, NOT_PROMPTS]);

    assert_eq!(
        spoken(&dir),
        [
            (Role::Prompt, "then this".to_string()),
            (Role::Reply, "Hello from the fake model.".to_string()),
            (Role::Prompt, "say hi".to_string()),
        ]
    );
}

#[test]
fn a_missing_file_has_no_message() {
    let dir = TempDir::new();

    assert_eq!(last_message(dir.path()).expect("read"), None);
}

#[test]
fn the_search_can_stop_early() {
    let dir = session(&[TURN, QUEUED]);
    let mut seen = 0;

    said_all(dir.path(), &mut |_| {
        seen += 1;
        false
    })
    .expect("read");

    assert_eq!(seen, 1);
}
