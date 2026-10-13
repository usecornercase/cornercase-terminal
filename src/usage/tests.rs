use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, write_executable};

const CLOCK: i64 = 1_791_158_400;

#[rstest]
#[case::minutes(13 * 60, "resets in 13m")]
#[case::rounds_up(30, "resets in 1m")]
#[case::hours(2 * 3_600 + 13 * 60, "resets in 2h 13m")]
#[case::days(3 * 86_400 + 4 * 3_600, "resets in 3d 4h")]
#[case::past(-5, "resets in 0m")]
fn reset_times_are_relative(#[case] secs: i64, #[case] expected: &str) {
    assert_eq!(until(secs), expected);
}

fn rows(section: &ui::UsageSection) -> Vec<(String, u16, String)> {
    section.windows.iter().map(|w| (w.label.clone(), w.percent, w.resets.clone())).collect()
}

#[test]
fn the_view_shows_loading_then_the_windows_and_their_age() {
    let mut state = State::default();
    let now = Instant::now();
    assert_eq!(state.start(vec![Agent::Claude]), [Agent::Claude]);
    assert_eq!(state.start(vec![Agent::Claude]), []);
    let loading = state.view(now, CLOCK);
    assert_eq!((loading.sections[0].windows.len(), loading.sections[0].status.as_str()), (0, "loading…"));

    state.answered(Agent::Claude, Ok(claude::tests::report(claude::tests::RECORDED)), now);
    let clock = time::parse_time("2026-10-04T11:46:59").expect("a time");
    let view = state.view(now + Duration::from_secs(120), clock);
    let section = &view.sections[0];
    assert_eq!(
        (section.title.as_str(), rows(section), section.status.as_str(), &section.error),
        (
            "Claude Code · team plan",
            vec![
                ("session (5h)".into(), 7, "resets in 2h 13m".into()),
                ("week".into(), 74, "resets in 4h 13m".into()),
                ("week · Fable".into(), 0, "resets in 4h 14m".into()),
            ],
            "updated 2m ago",
            &None,
        )
    );
}

#[test]
fn each_shown_agent_gets_its_own_section() {
    let mut state = State::default();
    let now = Instant::now();
    assert_eq!(state.start(Agent::ALL.to_vec()), Agent::ALL);
    state.answered(Agent::Codex, Ok(codex::tests::report(codex::tests::RECORDED)), now);
    let view = state.view(now, 1_791_218_970 - 3_600);
    let found: Vec<(&str, &str, usize)> =
        view.sections.iter().map(|s| (s.title.as_str(), s.status.as_str(), s.windows.len())).collect();
    assert_eq!(found, [("Claude Code", "loading…", 0), ("Codex · plus plan", "updated just now", 2)]);
    assert_eq!(rows(&view.sections[1])[0], ("session (5h)".into(), 53, "resets in 1h 0m".into()));
}

#[test]
fn only_the_shown_agents_have_a_section() {
    let mut state = State::default();
    state.start(vec![Agent::Codex]);
    let titles: Vec<String> = state.view(Instant::now(), CLOCK).sections.into_iter().map(|s| s.title).collect();
    assert_eq!(titles, ["Codex"]);
}

#[test]
fn a_failure_keeps_the_last_answer() {
    let mut state = State::default();
    let now = Instant::now();
    state.start(vec![Agent::Claude]);
    state.answered(Agent::Claude, Ok(claude::tests::report(claude::tests::RECORDED)), now);
    state.start(vec![Agent::Claude]);
    state.answered(Agent::Claude, Err(Error::Usage("claude did not answer in time".into())), now);
    let section = &state.view(now, CLOCK).sections[0];
    assert_eq!(
        (section.windows.len(), section.error.as_deref()),
        (3, Some("usage unavailable: claude did not answer in time"))
    );
}

#[rstest]
#[case::claude(Agent::Claude, "no plan limits for this account (API key, Bedrock or Vertex)")]
#[case::codex(Agent::Codex, "no plan limits for this account")]
fn an_account_without_limits_says_so(#[case] agent: Agent, #[case] expected: &str) {
    let mut state = State::default();
    state.start(vec![agent]);
    state.answered(agent, Ok(Report::default()), Instant::now());
    assert_eq!(state.view(Instant::now(), CLOCK).sections[0].empty, Some(expected));
}

#[test]
fn a_codex_without_a_login_says_how_to_sign_in_without_an_error() {
    let mut state = State::default();
    state.start(vec![Agent::Codex]);
    let signed_out = codex::tests::report(codex::tests::SIGNED_OUT);
    state.answered(Agent::Codex, Ok(signed_out), Instant::now());
    let section = &state.view(Instant::now(), CLOCK).sections[0];
    assert_eq!(
        (section.title.as_str(), section.empty, &section.error),
        ("Codex", Some("not signed in · run codex login"), &None)
    );
}

fn fake(name: &str, script: &str) -> (TempDir, String) {
    let dir = TempDir::new();
    let path = dir.path().join(name);
    write_executable(&path, &format!("#!/bin/sh\n{script}\n"));
    let command = path.display().to_string();
    (dir, command)
}

#[rstest]
#[case::claude(Agent::Claude, 2, claude::tests::RECORDED, claude::tests::report)]
#[case::codex(Agent::Codex, 3, codex::tests::RECORDED, codex::tests::report)]
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
