use std::time::{Duration, Instant};

use rstest::rstest;

use super::view;
use crate::error::Error;
use crate::time;
use crate::ui;
use crate::usage::{self, Agent, Report, State};

const CLOCK: i64 = 1_791_158_400;

fn rows(section: &ui::UsageSection) -> Vec<(String, u16, String)> {
    section.windows.iter().map(|w| (w.label.clone(), w.percent, w.resets.clone())).collect()
}

#[test]
fn the_view_shows_loading_then_the_windows_and_their_age() {
    let mut state = State::default();
    let now = Instant::now();
    assert_eq!(state.start(vec![Agent::Claude]), [Agent::Claude]);
    assert_eq!(state.start(vec![Agent::Claude]), []);
    let loading = view(&state, now, CLOCK);
    assert_eq!((loading.sections[0].windows.len(), loading.sections[0].status.as_str()), (0, "loading…"));

    state.answered(Agent::Claude, Ok(usage::claude_tests::report(usage::claude_tests::RECORDED)), now);
    let clock = time::parse_time("2026-10-04T11:46:59").expect("a time");
    let view = view(&state, now + Duration::from_secs(120), clock);
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
    state.answered(Agent::Codex, Ok(usage::codex_tests::report(usage::codex_tests::RECORDED)), now);
    let view = view(&state, now, 1_791_218_970 - 3_600);
    let found: Vec<(&str, &str, usize)> =
        view.sections.iter().map(|s| (s.title.as_str(), s.status.as_str(), s.windows.len())).collect();
    assert_eq!(found, [("Claude Code", "loading…", 0), ("Codex · plus plan", "updated just now", 2)]);
    assert_eq!(rows(&view.sections[1])[0], ("session (5h)".into(), 53, "resets in 1h 0m".into()));
}

#[test]
fn only_the_shown_agents_have_a_section() {
    let mut state = State::default();
    state.start(vec![Agent::Codex]);
    let titles: Vec<String> = view(&state, Instant::now(), CLOCK).sections.into_iter().map(|s| s.title).collect();
    assert_eq!(titles, ["Codex"]);
}

#[test]
fn a_failure_keeps_the_last_answer() {
    let mut state = State::default();
    let now = Instant::now();
    state.start(vec![Agent::Claude]);
    state.answered(Agent::Claude, Ok(usage::claude_tests::report(usage::claude_tests::RECORDED)), now);
    state.start(vec![Agent::Claude]);
    state.answered(Agent::Claude, Err(Error::Usage("claude did not answer in time".into())), now);
    let section = &view(&state, now, CLOCK).sections[0];
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
    assert_eq!(view(&state, Instant::now(), CLOCK).sections[0].empty, Some(expected));
}

#[test]
fn a_codex_without_a_login_says_how_to_sign_in_without_an_error() {
    let mut state = State::default();
    state.start(vec![Agent::Codex]);
    let signed_out = usage::codex_tests::report(usage::codex_tests::SIGNED_OUT);
    state.answered(Agent::Codex, Ok(signed_out), Instant::now());
    let section = &view(&state, Instant::now(), CLOCK).sections[0];
    assert_eq!(
        (section.title.as_str(), section.empty, &section.error),
        ("Codex", Some("not signed in · run codex login"), &None)
    );
}
