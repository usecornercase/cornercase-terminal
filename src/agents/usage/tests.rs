use rstest::rstest;

use super::*;
use crate::agents::claude::usage::tests as claude_tests;
use crate::agents::codex::usage::tests as codex_tests;
use crate::test_util::{TempDir, write_executable};

#[rstest]
#[case::minutes(13 * 60, "resets in 13m")]
#[case::rounds_up(30, "resets in 1m")]
#[case::hours(2 * 3_600 + 13 * 60, "resets in 2h 13m")]
#[case::days(3 * 86_400 + 4 * 3_600, "resets in 3d 4h")]
#[case::past(-5, "resets in 0m")]
fn reset_times_are_relative(#[case] secs: i64, #[case] expected: &str) {
    assert_eq!(until(secs), expected);
}

fn fake(name: &str, script: &str) -> (TempDir, String) {
    let dir = TempDir::new();
    let path = dir.path().join(name);
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    let command = path.display().to_string();
    (dir, command)
}

#[rstest]
#[case::claude(Agent::Claude, 2, claude_tests::RECORDED, claude_tests::report)]
#[case::codex(Agent::Codex, 3, codex_tests::RECORDED, codex_tests::report)]
fn the_probe_answers_after_reading_the_requests(
    #[case] agent: Agent,
    #[case] requests: usize,
    #[case] recorded: &str,
    #[case] report: fn(&str) -> Report,
) {
    let reads = "read -r line\n".repeat(requests);
    let script = format!("{reads}echo '{{\"type\":\"system\"}}'\necho '{recorded}'\nexec sleep 30");
    let (_dir, command) = fake(agent.kind(), &script);
    assert_eq!(probe(agent, &command, TIMEOUT).expect("a report"), report(recorded));
}

#[test]
fn the_codex_probe_runs_the_app_server() {
    let (_dir, command) = fake(
        "codex",
        "read -r a\nread -r b\nread -r c\necho \"{\\\"id\\\":2,\\\"error\\\":{\\\"message\\\":\\\"$1\\\"}}\"",
    );
    let error = probe(Agent::Codex, &command, TIMEOUT).expect_err("the arguments as an error");
    assert_eq!(error.to_string(), "app-server");
}

#[rstest]
#[case::hangs(Agent::Claude, "exec sleep 30", Duration::from_millis(300), "claude did not answer in time")]
#[case::exits(Agent::Claude, "exit 1", TIMEOUT, "claude exited without answering")]
#[case::codex_hangs(Agent::Codex, "exec sleep 30", Duration::from_millis(300), "codex did not answer in time")]
fn a_probe_that_gets_no_answer_fails(
    #[case] agent: Agent,
    #[case] script: &str,
    #[case] timeout: Duration,
    #[case] expected: &str,
) {
    let (_dir, command) = fake(agent.kind(), script);
    let error = probe(agent, &command, timeout).expect_err("no report");
    assert_eq!(error.to_string(), expected);
}

#[test]
fn an_agent_gone_before_the_requests_fails_at_once() {
    let gone = Some(Err(std::io::ErrorKind::BrokenPipe.into()));
    let error = await_answer(gone, None::<&'static [u8]>, claude::answer, "claude", Duration::from_secs(2))
        .expect_err("no report");
    assert_eq!(error.to_string(), "claude exited without answering");
}

#[test]
fn a_missing_agent_fails() {
    let error = probe(Agent::Codex, "/nonexistent/codex", TIMEOUT).expect_err("no report");
    assert!(error.to_string().starts_with("could not run /nonexistent/codex"), "{error}");
}
