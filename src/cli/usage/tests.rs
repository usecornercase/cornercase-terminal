use clap::Parser;
use rstest::rstest;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};
use crate::control::usage::{LimitInfo, MachineRead, MachineUsage};
use crate::machines::State;

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
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
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
        ..Plans::default()
    };
    assert_eq!(
        text(&plans.agents, clock()),
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
    let never = Plans { agents: vec![PlanInfo { title: "Codex".into(), ..PlanInfo::default() }], ..Plans::default() };
    assert_eq!(text(&never.agents, 0), "Codex\n  not asked yet\n");
    let loading = Plans {
        agents: vec![PlanInfo { title: "Claude Code".into(), loading: true, ..PlanInfo::default() }],
        ..Plans::default()
    };
    assert_eq!(text(&loading.agents, 0), "Claude Code  loading…\n");
}

#[test]
fn the_flags_parse() {
    let cli = Cli::try_parse_from(["cornercase", "usage", "--cached", "--json"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Usage(args))) => assert!(args.cached && args.print.json),
        other => panic!("not usage: {other:?}"),
    }
}

#[test]
fn an_account_is_named_and_the_active_one_marked() {
    let plan = |account: Option<&str>, label: Option<&str>, active: bool| {
        PlanInfo { title: "Claude Code".into(), ..PlanInfo::default() }.for_account(
            account,
            label.map(str::to_string),
            active,
        )
    };
    let plans = Plans {
        agents: vec![
            plan(None, Some("System"), false),
            plan(Some("work"), None, false),
            plan(Some("home"), Some("Personal"), true),
        ],
        ..Plans::default()
    };
    assert_eq!(
        text(&plans.agents, clock()),
        "Claude Code · System\n  not asked yet\n\nClaude Code · work\n  not asked yet\n\n● Claude Code · Personal\n  not asked yet\n"
    );
}

fn read(name: &str, read_at: Option<&str>, error: Option<&str>) -> MachineRead {
    MachineRead {
        name: name.into(),
        state: if error.is_some() { State::Unreachable } else { State::Linked },
        read_at: read_at.map(str::to_string),
        error: error.map(str::to_string),
        ..MachineRead::default()
    }
}

fn codex(error: Option<&str>) -> PlanInfo {
    PlanInfo {
        title: "Codex".into(),
        read_at: Some("x".into()),
        error: error.map(str::to_string),
        ..PlanInfo::default()
    }
}

const READ_AT: &str = "2026-10-04T11:40:00.000Z";

mod the_machine_flag {
    use super::*;

    #[rstest]
    #[case::a_name(&["usage", "--machine", "vps"], Some("vps"))]
    #[case::every_machine(&["usage", "--machine", "all", "--cached"], Some("all"))]
    #[case::none(&["usage"], None)]
    fn is_sent_as_given(#[case] args: &[&str], #[case] machine: Option<&str>) {
        let cli = Cli::try_parse_from(std::iter::once("cornercase").chain(args.iter().copied())).expect("parse");
        match cli.command {
            Some(Command::Control(Control::Usage(args))) => assert_eq!(args.machine.as_deref(), machine),
            other => panic!("not usage: {other:?}"),
        }
    }

    #[test]
    fn needs_a_name() {
        assert!(Cli::try_parse_from(["cornercase", "usage", "--machine", ""]).is_err());
    }
}

mod the_text_of_machines {
    use super::*;

    #[test]
    fn heads_one_machine_with_when_it_was_read() {
        let plans =
            Plans { agents: vec![codex(None)], machine: Some(read("vps", Some(READ_AT), None)), ..Plans::default() };

        assert_eq!(every_text(&plans, clock()), "● vps  read 6 min ago\nCodex\n");
    }

    #[test]
    fn lists_this_computer_then_each_machine() {
        let plans = Plans {
            agents: vec![codex(None)],
            machines: vec![
                MachineUsage { machine: read("vps", Some(READ_AT), None), agents: vec![codex(None)] },
                MachineUsage { machine: read("pi", None, Some("ssh: Connection refused")), agents: Vec::new() },
            ],
            ..Plans::default()
        };

        assert_eq!(
            every_text(&plans, clock()),
            "▭ this computer\nCodex\n\n● vps  read 6 min ago\nCodex\n\n○ pi  ssh: Connection refused\n"
        );
    }

    #[test]
    fn is_the_plain_text_without_machines() {
        let plans = Plans { agents: vec![codex(None)], ..Plans::default() };

        assert_eq!(every_text(&plans, clock()), text(&plans.agents, clock()));
    }
}

mod the_answer {
    use super::*;

    fn every(here: Option<&str>, there: Option<&str>, machine: Option<&str>) -> Plans {
        let machines = vec![MachineUsage { machine: read("vps", Some(READ_AT), machine), agents: vec![codex(there)] }];
        Plans { agents: vec![codex(here)], machines, ..Plans::default() }
    }

    #[rstest]
    #[case::every_one_read(every(None, None, None), true)]
    #[case::an_agent_here_failed(every(Some("no"), None, None), false)]
    #[case::an_agent_there_failed(every(None, Some("no"), None), false)]
    #[case::a_machine_failed(every(None, None, Some("down")), false)]
    #[case::a_machine_never_read(
        Plans { machines: vec![MachineUsage { machine: read("vps", None, None), agents: Vec::new() }], ..Plans::default() },
        false
    )]
    #[case::the_one_machine_failed(
        Plans { machine: Some(read("vps", Some(READ_AT), Some("down"))), ..Plans::default() },
        false
    )]
    fn is_whole_only_when_no_machine_nor_agent_failed(#[case] plans: Plans, #[case] whole: bool) {
        assert_eq!(answered(&plans), whole);
    }
}
