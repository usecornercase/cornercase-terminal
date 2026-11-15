use super::*;
use crate::agents::headless::{feed, plain_args, steps_of};
use crate::test_util::fixture;

fn argv(tools: Tools, conversation: Option<&str>) -> Vec<String> {
    plain_args(command, tools, conversation)
}

#[test]
fn a_first_turn_gives_its_session_the_steps_and_only_the_last_answer() {
    let (ran, last) = feed(read, fixture!("cursor/2026.10.22-27dcf5f/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("4a7c2e91-0b3d-4f58-8e16-a9d2c5b70f34"));
    assert_eq!(
        steps_of(&ran),
        [
            ("shell", "cornercase --version"),
            ("edit", "edit /work/bots/prs/inside.txt"),
            ("edit", "edit /work/bots/prs/bot.json"),
        ]
    );
    assert_eq!(ran.text, "Steps 1, 2, 3, and 5 worked.");
    assert_eq!(ran.tokens, Used { input: 11898, output: 952, cache_read: 125_952, cache_write: 0 });
    assert_eq!(ran.model.as_deref(), Some("Grok 4.7 500K Extra High Fast"));
}

#[test]
fn the_answer_is_not_the_joined_result_text() {
    let (ran, _) = feed(read, fixture!("cursor/2026.10.22-27dcf5f/bot-new.jsonl"));
    assert!(!ran.text.contains("Starting with"), "{}", ran.text);
}

#[test]
fn a_resumed_turn_names_the_same_session() {
    let (ran, last) = feed(read, fixture!("cursor/2026.10.22-27dcf5f/bot-resumed.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("4a7c2e91-0b3d-4f58-8e16-a9d2c5b70f34"));
    assert_eq!(ran.text, "You asked me to say hi in one short sentence.");
}

#[test]
fn a_result_without_assistant_text_answers_with_its_own() {
    let (ran, last) =
        feed(read, r#"{"type":"result","subtype":"success","is_error":false,"result":" Done. ","usage":{}}"#);
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "Done.");
}

#[test]
fn an_error_result_fails_with_its_text() {
    let (_, last) = feed(read, r#"{"type":"result","subtype":"error","is_error":true,"result":"Quota exceeded"}"#);
    assert_eq!(last.expect("a result").expect_err("failed").to_string(), "Quota exceeded");
}

#[test]
fn a_turn_without_an_answer_is_an_error() {
    let (_, last) = feed(read, r#"{"type":"result","subtype":"success","is_error":false,"result":""}"#);
    assert_eq!(last.expect("a result").expect_err("empty").to_string(), "Cursor CLI answered nothing");
}

#[test]
fn it_always_trusts_the_folder_and_only_full_access_forces() {
    let full = argv(Tools::Bot(Access::Full), Some("c-1"));
    assert_eq!(
        full,
        ["-p", "--output-format", "stream-json", "--trust", "--force", "--resume", "c-1", "--model", "m", "--", "- hi"]
    );
    let other = argv(Tools::Bot(Access::Cornercase), None);
    assert_eq!(other, ["-p", "--output-format", "stream-json", "--trust", "--model", "m", "--", "- hi"]);
}
