use std::time::Duration;

use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::agents::usage::{Report, State};
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request, Stats};
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
    state.start(vec![(agent, None)]);
    state.answered((agent, None), Ok(report), at);
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
        identity: None,
    };
    read(&mut state, Known::Claude, report, at);
    state.start(vec![(Known::Codex, None)]);
    state.answered((Known::Codex, None), Err(Error::Usage("could not run /nonexistent/codex".into())), at);
    let now = at + Duration::from_secs(120);
    Plans {
        agents: [Known::Claude, Known::Codex]
            .into_iter()
            .map(|agent| PlanInfo::of(agent, state.probe(agent), now, clock()))
            .collect(),
        ..Plans::default()
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
    assert_eq!(parsed.command, Command::Usage(Usage { cached: true, machine: None }));
    assert_eq!(unknown_fields(&raw, &parsed), None);
    let raw = json!({"command": "usage", "args": {"cached": true, "bogus": 1}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("request");
    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.bogus"));
}

#[test]
fn the_logged_fields_are_the_flag() {
    let mut fields = Fields::default();
    Usage { cached: true, machine: None }.fields(&mut fields);
    assert_eq!(fields.0, [("cached", "true".to_string())]);
}

#[test]
fn nothing_read_gives_null_fields_and_a_start_says_loading() {
    let mut state = State::default();
    let now = Instant::now();
    let none = PlanInfo::of(Known::Claude, state.probe(Known::Claude), now, clock());
    assert_eq!(none, PlanInfo { agent: "claude".into(), title: "Claude Code".into(), ..PlanInfo::default() });
    assert_eq!(none.limited, None);
    state.start(vec![(Known::Claude, None)]);
    assert!(PlanInfo::of(Known::Claude, state.probe(Known::Claude), now, clock()).loading);
}

#[test]
fn a_later_error_keeps_the_reading() {
    let mut state = State::default();
    let at = Instant::now();
    read(&mut state, Known::Claude, Report { plan: Some("max".into()), limited: true, ..Report::default() }, at);
    state.start(vec![(Known::Claude, None)]);
    state.answered((Known::Claude, None), Err(Error::Usage("late".into())), at);
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

#[test]
fn an_account_entry_is_written_as_its_fixture_and_reads_back() {
    let mut plans = example();
    plans.agents[0] = plans.agents[0].clone().for_account(Some("work"), Some("Work".into()), true);
    assert_eq!(written(&plans), fixture!("control/usage/answer-accounts.json").trim_end());
    let read: Plans = serde_json::from_str(fixture!("control/usage/answer-accounts.json")).expect("plans");
    assert_eq!(read, plans);
    let old: Plans = serde_json::from_str(fixture!("control/usage/answer.json")).expect("plans");
    assert!(old.agents.iter().all(|plan| plan.account.is_none() && plan.label.is_none() && !plan.active));
}

mod the_machine_field {
    use super::*;

    fn usage(machine: Option<&str>) -> Command {
        Command::Usage(Usage { cached: true, machine: machine.map(Into::into) })
    }

    fn stats(machine: Option<&str>) -> Command {
        Command::Stats(Stats { machine: machine.map(Into::into) })
    }

    #[rstest]
    #[case::stats(stats(None), r#"{"command":"stats","args":{}}"#)]
    #[case::usage(usage(None), r#"{"command":"usage","args":{"cached":true}}"#)]
    fn is_left_out_of_a_request_without_it(#[case] command: Command, #[case] sent: &str) {
        assert_eq!(serde_json::to_string(&command).expect("json"), sent);
    }

    #[rstest]
    #[case::stats(json!({"command": "stats", "args": {"machine": "vps"}}), stats(Some("vps")))]
    #[case::usage(json!({"command": "usage", "args": {"cached": true, "machine": "all"}}), usage(Some("all")))]
    fn is_read_and_is_no_stray_field(#[case] raw: Value, #[case] command: Command) {
        let parsed: Request = serde_json::from_value(raw.clone()).expect("request");

        assert_eq!(parsed.command, command);
        assert_eq!(unknown_fields(&raw, &parsed), None);
    }

    #[rstest]
    #[case::stats(stats(Some("vps")), &[("machine", "vps")])]
    #[case::stats_here(stats(None), &[])]
    #[case::usage(usage(Some("all")), &[("cached", "true"), ("machine", "all")])]
    fn is_logged(#[case] command: Command, #[case] logged: &[(&str, &str)]) {
        let fields = command.fields();

        let fields: Vec<(&str, &str)> = fields.iter().map(|(key, value)| (*key, value.as_str())).collect();
        assert_eq!(fields, logged);
    }
}

mod an_answer_for_machines {
    use super::*;

    fn vps() -> MachineRead {
        MachineRead {
            name: "vps".into(),
            state: crate::machines::State::Unreachable,
            seen: Some(READ.into()),
            read_at: Some(READ.into()),
            error: Some("ssh: connect to host vps port 22: Connection refused".into()),
        }
    }

    #[test]
    fn carries_the_identity_of_a_probe_only_when_it_has_one() {
        let mut state = State::default();
        let at = Instant::now();
        read(&mut state, Known::Claude, Report { identity: Some("1a2b".into()), ..Report::default() }, at);

        let info = PlanInfo::of(Known::Claude, state.probe(Known::Claude), at, clock());

        assert_eq!(info.identity.as_deref(), Some("1a2b"));
        assert_eq!(serde_json::to_value(&info).expect("json")["identity"], "1a2b");
        assert!(!written(&example()).contains("identity"));
    }

    #[test]
    fn of_stats_is_the_view_with_the_machine_beside_it() {
        let view = View::default();
        let here = serde_json::to_value(&view).expect("json");

        let alone = MachineStats { view: view.clone(), ..MachineStats::default() };
        let one = MachineStats { view, machine: Some(vps()), machines: Vec::new() };

        assert_eq!(serde_json::to_value(&alone).expect("json"), here);
        let one = serde_json::to_value(&one).expect("json");
        assert_eq!((&one["since"], &one["windows"]), (&here["since"], &here["windows"]));
        assert_eq!(one["machine"], serde_json::to_value(vps()).expect("json"));
        let read: MachineStats = serde_json::from_value(one).expect("stats");
        assert_eq!(read.machine, Some(vps()));
    }

    #[test]
    fn of_usage_lists_each_machine_with_its_agents_and_reads_back() {
        let agents = example().agents;
        let plans = Plans { machines: vec![MachineUsage { machine: vps(), agents: agents.clone() }], ..example() };

        let value = serde_json::to_value(&plans).expect("json");

        assert_eq!(value["machines"][0]["name"], "vps");
        assert_eq!(value["machines"][0]["state"], "unreachable");
        assert_eq!(value["machines"][0]["agents"].as_array().map(Vec::len), Some(agents.len()));
        assert_eq!(serde_json::from_value::<Plans>(value).expect("plans"), plans);
    }
}
