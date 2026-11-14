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
fn a_real_run_gives_the_last_message_and_the_tokens() {
    let (ran, last) = feed(fixture!("codex/0.164.0/recap-ok.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.text, "hello");
    assert_eq!(ran.steps, 1);
    assert_eq!(ran.tokens.cache_read, 27008);
    assert_eq!(ran.tokens.input, 29614 - 27008);
}

#[test]
fn a_failed_turn_fails_with_the_message() {
    let (_, last) = feed(fixture!("codex/0.164.0/recap-failed.jsonl"));
    assert!(last.expect("a result").expect_err("failed").to_string().contains("usage limit"));
}

#[test]
fn a_turn_without_a_message_is_an_error() {
    let (_, last) = feed(r#"{"type":"turn.completed","usage":{}}"#);
    assert!(last.expect("a result").is_err());
}

#[test]
fn a_bot_run_gives_its_thread_and_the_command_without_its_shell() {
    let (ran, last) = feed(fixture!("codex/0.164.0/bot-new.jsonl"));
    assert!(last.expect("a result").is_ok());
    assert_eq!(ran.session.as_deref(), Some("01a1da0e-8fb3-7432-99ff-2484ead47afc"));
    assert_eq!(ran.fresh.len(), 1);
    assert_eq!((ran.fresh[0].tool.as_str(), ran.fresh[0].text.as_str()), ("shell", "cornercase status --json"));
}

#[test]
fn a_resumed_run_names_the_same_thread() {
    let (ran, _) = feed(fixture!("codex/0.164.0/bot-resumed.jsonl"));
    assert_eq!(ran.session.as_deref(), Some("01a1da0e-8fb3-7432-99ff-2484ead47afc"));
}

#[rstest::rstest]
#[case::zsh("/usr/bin/zsh -lc 'cat dossier.md'", "cat dossier.md")]
#[case::bash("bash -c \"ls -la\"", "ls -la")]
#[case::bare("git status", "git status")]
fn a_shell_wrapper_is_taken_off_a_command(#[case] command: &str, #[case] wanted: &str) {
    assert_eq!(unwrap_shell(command), wanted);
}
