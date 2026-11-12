use serde_json::json;

use super::*;
use crate::test_util::TempDir;

fn prompt(text: &str) -> String {
    json!({"type": "user", "origin": {"kind": "human"}, "message": {"content": text}}).to_string()
}

fn reply(id: usize, text: &str, second: usize) -> String {
    json!({
        "type": "assistant", "isSidechain": false,
        "timestamp": format!("2026-11-12T09:{:02}:{:02}.000Z", second / 60, second % 60),
        "message": {"id": format!("m{id}"), "model": "claude-opus-5-5", "role": "assistant",
                    "content": [{"type": "text", "text": text}]},
    })
    .to_string()
}

fn talk(lines: &[String]) -> Talk {
    let dir = TempDir::new();
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, lines.join("\n") + "\n").expect("transcript");
    read(&Record::Claude(path)).expect("read")
}

#[test]
fn messages_come_oldest_first_with_prompts_and_replies() {
    let tool = json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": "ok"}]}}).to_string();

    let talk = talk(&[prompt("fix it"), reply(1, "looking", 1), tool, reply(2, "fixed", 2)]);

    let said: Vec<(Role, &str)> = talk.messages.iter().map(|m| (m.role, m.text.as_str())).collect();
    assert_eq!(said, [(Role::Prompt, "fix it"), (Role::Reply, "looking"), (Role::Reply, "fixed")]);
    assert!(!talk.earlier);
}

#[test]
fn only_the_newest_thousand_stay_and_earlier_says_so() {
    let lines: Vec<String> = (0..=MESSAGES).map(|n| reply(n, &format!("reply {n}"), n % 3600)).collect();

    let talk = talk(&lines);

    assert_eq!(talk.messages.len(), MESSAGES);
    assert!(talk.earlier);
    assert_eq!(talk.messages.last().map(|m| m.text.as_str()), Some("reply 1000"));
    assert_eq!(talk.messages.first().map(|m| m.text.as_str()), Some("reply 1"));
}

#[test]
fn a_long_message_is_cut_at_thirty_two_kib() {
    let talk = talk(&[reply(1, &"a".repeat(MESSAGE_CAP + 10), 1)]);

    assert_eq!((talk.messages[0].text.len(), talk.messages[0].cut), (MESSAGE_CAP, true));
}

#[test]
fn a_missing_record_is_an_empty_talk() {
    let dir = TempDir::new();

    let talk = read(&Record::Claude(dir.path().join("none.jsonl"))).expect("read");

    assert_eq!(talk, Talk::default());
}

#[test]
fn a_vibe_journal_that_reads_oldest_first_keeps_only_the_newest_thousand() {
    let fake = crate::agents::vibe::fake::FakeVibe::new();
    let records: Vec<serde_json::Value> = (0..=MESSAGES + 5)
        .map(|n| {
            json!({"type": "action_result", "payload": {"result": {"result": {
                "parts": [{"type": "text", "text": format!("reply {n}")}]}}}})
        })
        .collect();
    fake.journal(&records);

    let talk = read(&Record::Vibe(fake.session())).expect("read");

    assert_eq!(talk.messages.len(), MESSAGES);
    assert!(talk.earlier);
    assert_eq!(talk.messages.first().map(|m| m.text.as_str()), Some("reply 6"));
    assert_eq!(talk.messages.last().map(|m| m.text.as_str()), Some("reply 1005"));
}
