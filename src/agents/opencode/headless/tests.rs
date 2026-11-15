use super::*;
use crate::agents::headless::{feed, plain_args, steps_of};
use crate::test_util::fixture;

fn argv(tools: Tools, conversation: Option<&str>) -> Vec<String> {
    plain_args(command, tools, conversation)
}

#[test]
fn a_first_turn_gives_its_session_the_steps_and_only_the_last_answer() {
    let (ran, last) = feed(read, fixture!("opencode/1.18.35/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
    assert_eq!(
        steps_of(&ran),
        [
            ("bash", "cornercase --version"),
            ("write", "write /work/bots/prs/inside.txt"),
            ("write", "write /work/outside/outside.txt"),
            ("write", "write /work/bots/prs/bot.json"),
        ]
    );
    assert_eq!(ran.text, "Steps 1, 2, 5 worked; steps 3, 4, 6, 7 were refused.");
    assert_eq!(
        ran.tokens,
        Used {
            input: 6523 + 102 + 69 + 2514 + 131,
            output: 85 + 79 + 79 + 78 + 27,
            cache_read: 4096 + 10624 + 10752 + 16000 + 18560,
            cache_write: 0
        }
    );
    assert_eq!(ran.model, None);
}

#[test]
fn a_resumed_turn_names_the_same_session() {
    let (ran, last) = feed(read, fixture!("opencode/1.18.35/bot-resumed.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("ses_e2a1b2c3d4ffeAbCdEfGhIjKlM"));
    assert_eq!(ran.text, "You asked me to say hi in one short sentence.");
    assert_eq!(ran.tokens.output, 12 + 12);
}

#[test]
fn a_failed_turn_fails_with_the_agents_words() {
    let (_, last) = feed(read, fixture!("opencode/1.18.35/bot-failed.jsonl"));
    let error = last.expect("a result").expect_err("failed");
    assert!(error.to_string().contains("Unexpected server error"), "{error}");
}

#[test]
fn a_turn_without_an_answer_is_an_error() {
    let (_, last) = feed(read, r#"{"type":"step_finish","sessionID":"s","part":{"reason":"stop","tokens":{}}}"#);
    assert_eq!(last.expect("a result").expect_err("empty").to_string(), "opencode answered nothing");
}

#[test]
fn a_step_that_calls_tools_does_not_end_the_turn() {
    let (_, last) = feed(read, r#"{"type":"step_finish","sessionID":"s","part":{"reason":"tool-calls","tokens":{}}}"#);
    assert!(last.is_none());
}

#[test]
fn only_full_access_bypasses_permissions_and_a_session_is_continued() {
    let full = argv(Tools::Bot(Access::Full), Some("ses_1"));
    assert_eq!(full, ["run", "--format", "json", "--auto", "--session", "ses_1", "--model", "m", "--", "- hi"]);
    let other = argv(Tools::Bot(Access::Cornercase), None);
    assert_eq!(other, ["run", "--format", "json", "--model", "m", "--", "- hi"]);
}
