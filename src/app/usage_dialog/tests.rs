use std::time::{Duration, Instant};

use rstest::rstest;

use super::sections;
use crate::agents::Known;
use crate::agents::claude::usage::tests as claude_tests;
use crate::agents::codex::usage::tests as codex_tests;
use crate::agents::usage::{Report, State};
use crate::error::Error;
use crate::time;
use crate::ui;

const CLOCK: i64 = 1_791_158_400;

fn rows(section: &ui::UsageSection) -> Vec<(String, u16, String)> {
    section.windows.iter().map(|w| (w.label.clone(), w.percent, w.resets.clone())).collect()
}

#[test]
fn the_view_shows_loading_then_the_windows_and_their_age() {
    let mut state = State::default();
    let now = Instant::now();
    assert_eq!(state.start(vec![Known::Claude]), [Known::Claude]);
    assert_eq!(state.start(vec![Known::Claude]), []);
    let loading = sections(&state, now, CLOCK);
    assert_eq!((loading[0].windows.len(), loading[0].status.as_str()), (0, "loading…"));

    state.answered(Known::Claude, Ok(claude_tests::report(claude_tests::RECORDED)), now);
    let clock = time::parse_time("2026-10-04T11:46:59").expect("a time");
    let view = sections(&state, now + Duration::from_secs(120), clock);
    let section = &view[0];
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
    assert_eq!(state.start(vec![Known::Claude, Known::Codex]), [Known::Claude, Known::Codex]);
    state.answered(Known::Codex, Ok(codex_tests::report(codex_tests::RECORDED)), now);
    let view = sections(&state, now, 1_791_218_970 - 3_600);
    let found: Vec<(&str, &str, usize)> =
        view.iter().map(|s| (s.title.as_str(), s.status.as_str(), s.windows.len())).collect();
    assert_eq!(found, [("Claude Code", "loading…", 0), ("Codex · plus plan", "updated just now", 2)]);
    assert_eq!(rows(&view[1])[0], ("session (5h)".into(), 53, "resets in 1h 0m".into()));
}

#[test]
fn only_the_shown_agents_have_a_section() {
    let mut state = State::default();
    state.start(vec![Known::Codex]);
    let titles: Vec<String> = sections(&state, Instant::now(), CLOCK).into_iter().map(|s| s.title).collect();
    assert_eq!(titles, ["Codex"]);
}

#[test]
fn a_failure_keeps_the_last_answer() {
    let mut state = State::default();
    let now = Instant::now();
    state.start(vec![Known::Claude]);
    state.answered(Known::Claude, Ok(claude_tests::report(claude_tests::RECORDED)), now);
    state.start(vec![Known::Claude]);
    state.answered(Known::Claude, Err(Error::Usage("claude did not answer in time".into())), now);
    let section = &sections(&state, now, CLOCK)[0];
    assert_eq!(
        (section.windows.len(), section.error.as_deref()),
        (3, Some("usage unavailable: claude did not answer in time"))
    );
}

#[rstest]
#[case::claude(Known::Claude, "no plan limits for this account (API key, Bedrock or Vertex)")]
#[case::codex(Known::Codex, "no plan limits for this account")]
fn an_account_without_limits_says_so(#[case] agent: Known, #[case] expected: &str) {
    let mut state = State::default();
    state.start(vec![agent]);
    state.answered(agent, Ok(Report::default()), Instant::now());
    assert_eq!(sections(&state, Instant::now(), CLOCK)[0].empty, Some(expected));
}

#[test]
fn a_codex_without_a_login_says_how_to_sign_in_without_an_error() {
    let mut state = State::default();
    state.start(vec![Known::Codex]);
    let signed_out = codex_tests::report(codex_tests::SIGNED_OUT);
    state.answered(Known::Codex, Ok(signed_out), Instant::now());
    let section = &sections(&state, Instant::now(), CLOCK)[0];
    assert_eq!(
        (section.title.as_str(), section.empty, &section.error),
        ("Codex", Some("not signed in · run codex login"), &None)
    );
}
