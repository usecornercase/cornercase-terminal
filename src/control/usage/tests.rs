use std::time::Duration;

use serde_json::json;

use super::*;
use crate::agents::usage::{Report, State};
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::error::Error;
use crate::test_util::fixture;

const READ: &str = "2026-10-04T11:44:59.000Z";

fn clock() -> SystemTime {
    Stamp::parse("2026-10-04T11:46:59.000Z").expect("a time")
}

fn reset(text: &str) -> i64 {
    Stamp::parse(text)
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .expect("a reset")
}

fn window(label: &str, kind: &str, scope: Option<&str>, percent: u16, at: &str) -> Window {
    Window {
        label: label.into(),
        kind: kind.into(),
        scope: scope.map(str::to_string),
        percent,
        severity: crate::agents::usage::Severity::of(percent),
        resets_at: Some(reset(at)),
    }
}

fn read(state: &mut State, agent: Known, report: Report, at: Instant) {
    state.start(vec![agent]);
    state.answered(agent, Ok(report), at);
}

fn example() -> Plans {
    let mut state = State::default();
    let at = Instant::now();
    let report = Report {
        plan: Some("team".into()),
        limited: true,
        windows: vec![
            window("session (5h)", "session", None, 7, "2026-10-04T13:59:59.000Z"),
            window("week · Fable", "weekly_scoped", Some("Fable"), 0, "2026-10-04T16:00:00.000Z"),
        ],
        extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
        notice: None,
    };
    read(&mut state, Known::Claude, report, at);
    state.start(vec![Known::Codex]);
    state.answered(Known::Codex, Err(Error::Usage("could not run /nonexistent/codex".into())), at);
    let now = at + Duration::from_secs(120);
    Plans {
        agents: [Known::Claude, Known::Codex]
            .into_iter()
            .map(|agent| PlanInfo::of(agent, state.probe(agent), now, clock()))
            .collect(),
    }
}

fn written(plans: &Plans) -> String {
    serde_json::to_string_pretty(plans).expect("json")
}

#[test]
fn the_answer_is_written_as_its_fixture() {
    assert_eq!(written(&example()), fixture!("control/usage/answer.json").trim_end());
}

#[test]
fn the_fixture_reads_back() {
    let read: Plans = serde_json::from_str(fixture!("control/usage/answer.json")).expect("plans");
    assert_eq!(read, example());
    assert_eq!(read.agents[0].read_at.as_deref(), Some(READ));
    assert_eq!(read.agents[0].age, Some(120));
}

#[test]
fn a_newer_server_may_add_keys_and_severities() {
    let read: Plans = serde_json::from_value(json!({
        "agents": [{"agent": "x", "limits": [{"label": "l", "severity": "dire", "new": 1}], "newer": true}],
        "extra": 1
    }))
    .expect("plans");
    assert_eq!(read.agents[0].limits[0].severity, "dire");
    assert_eq!(read.agents[0].limits[0].resets_at, None);
}

#[test]
fn the_request_parses_and_has_no_stray_field() {
    let raw = json!({"command": "usage", "args": {"cached": true}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(parsed.command, Command::Usage(Usage { cached: true }));
    assert_eq!(unknown_fields(&raw, &parsed), None);
    let raw = json!({"command": "usage", "args": {"cached": true, "bogus": 1}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.bogus"));
}

#[test]
fn the_logged_fields_are_the_flag() {
    let mut fields = Fields::default();
    Usage { cached: true }.fields(&mut fields);
    assert_eq!(fields.0, [("cached", "true".to_string())]);
}

#[test]
fn nothing_read_gives_null_fields_and_a_start_says_loading() {
    let mut state = State::default();
    let now = Instant::now();
    let none = PlanInfo::of(Known::Claude, state.probe(Known::Claude), now, clock());
    assert_eq!(none, PlanInfo { agent: "claude".into(), title: "Claude Code".into(), ..PlanInfo::default() });
    assert_eq!(none.limited, None);
    state.start(vec![Known::Claude]);
    assert!(PlanInfo::of(Known::Claude, state.probe(Known::Claude), now, clock()).loading);
}

#[test]
fn a_later_error_keeps_the_reading() {
    let mut state = State::default();
    let at = Instant::now();
    read(&mut state, Known::Claude, Report { plan: Some("max".into()), limited: true, ..Report::default() }, at);
    state.start(vec![Known::Claude]);
    state.answered(Known::Claude, Err(Error::Usage("late".into())), at);
    let info = PlanInfo::of(Known::Claude, state.probe(Known::Claude), at, clock());
    assert_eq!((info.plan.as_deref(), info.error.as_deref()), (Some("max"), Some("late")));
    assert!(info.read_at.is_some());
}

#[test]
fn the_notice_is_the_tabs_text() {
    let mut state = State::default();
    let at = Instant::now();
    read(&mut state, Known::Claude, Report::default(), at);
    let info = PlanInfo::of(Known::Claude, state.probe(Known::Claude), at, clock());
    assert_eq!(info.notice.as_deref(), Some("no plan limits for this account (API key, Bedrock or Vertex)"));
    let signed_out = Report { notice: Some("not signed in · run codex login"), ..Report::default() };
    read(&mut state, Known::Codex, signed_out, at);
    let info = PlanInfo::of(Known::Codex, state.probe(Known::Codex), at, clock());
    assert_eq!(info.notice.as_deref(), Some("not signed in · run codex login"));
}
