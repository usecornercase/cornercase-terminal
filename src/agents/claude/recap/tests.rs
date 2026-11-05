use super::*;
use crate::test_util::fixture;

fn feed(text: &str) -> (Ran, Option<Result<()>>) {
    let mut ran = Ran::default();
    let mut last = None;
    for line in text.lines() {
        last = read(line, &mut ran);
    }
    (ran, last)
}

#[test]
fn a_real_run_gives_the_text_the_model_and_the_tokens() {
    let (ran, last) = feed(fixture!("claude/2.1.318/recap-ok.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "hello");
    assert_eq!(ran.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(ran.steps, 1);
    assert_eq!(ran.tokens.output, 73);
}

#[test]
fn an_error_result_fails_with_its_text() {
    let (_, last) = feed(fixture!("claude/2.1.318/recap-failed.jsonl"));
    assert_eq!(last.expect("a result").expect_err("failed").to_string(), "Not logged in · Please run /login");
}

#[test]
fn an_empty_result_is_an_error() {
    let (_, last) = feed(r#"{"type":"result","subtype":"success","is_error":false,"result":"  "}"#);
    assert!(last.expect("a result").is_err());
}

#[test]
fn a_run_stopped_by_its_turns_is_an_error_named_by_its_subtype() {
    let (_, last) = feed(r#"{"type":"result","subtype":"error_max_turns","is_error":false}"#);
    assert_eq!(last.expect("a result").expect_err("stopped").to_string(), "error_max_turns");
}

#[test]
fn lines_that_are_not_json_or_unknown_are_passed() {
    let mut ran = Ran::default();
    assert!(read("not json", &mut ran).is_none());
    assert!(read(r#"{"type":"rate_limit_event"}"#, &mut ran).is_none());
}
