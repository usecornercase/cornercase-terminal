use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};
use crate::control::usage::LimitInfo;

fn limit(label: &str, percent: u16, resets_at: &str) -> LimitInfo {
    LimitInfo {
        label: label.into(),
        percent,
        severity: "normal".into(),
        resets_at: Some(resets_at.into()),
        ..LimitInfo::default()
    }
}

fn clock() -> i64 {
    i64::try_from(
        Stamp::parse("2026-10-04T11:46:00.000Z")
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .expect("a time")
            .as_secs(),
    )
    .expect("fits")
}

#[test]
fn the_text_follows_the_limits_tab() {
    let plans = Plans {
        agents: vec![
            PlanInfo {
                title: "Claude Code".into(),
                plan: Some("max".into()),
                age: Some(5),
                read_at: Some("x".into()),
                limits: vec![
                    limit("session (5h)", 34, "2026-10-04T14:00:00.000Z"),
                    limit("week", 81, "2026-10-07T16:00:00.000Z"),
                    limit("week · Opus", 12, "2026-10-07T16:00:00.000Z"),
                ],
                extra: Some("extra usage: 12.34 USD of 50.00 USD".into()),
                ..PlanInfo::default()
            },
            PlanInfo {
                title: "Codex".into(),
                error: Some("codex did not answer in time".into()),
                ..PlanInfo::default()
            },
        ],
    };
    assert_eq!(
        text(&plans, clock()),
        "Claude Code · max plan  updated just now
  session (5h)   34% · resets in 2h 14m
  week           81% · resets in 3d 4h
  week · Opus    12% · resets in 3d 4h
  extra usage: 12.34 USD of 50.00 USD

Codex
  usage unavailable: codex did not answer in time
"
    );
}

#[test]
fn an_agent_never_asked_or_still_loading_says_so() {
    let never = Plans { agents: vec![PlanInfo { title: "Codex".into(), ..PlanInfo::default() }] };
    assert_eq!(text(&never, 0), "Codex\n  not asked yet\n");
    let loading =
        Plans { agents: vec![PlanInfo { title: "Claude Code".into(), loading: true, ..PlanInfo::default() }] };
    assert_eq!(text(&loading, 0), "Claude Code  loading…\n");
}

#[test]
fn the_flags_parse() {
    let cli = Cli::try_parse_from(["cornercase", "usage", "--cached", "--json"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Usage(args))) => assert!(args.cached && args.print.json),
        other => panic!("not usage: {other:?}"),
    }
}
