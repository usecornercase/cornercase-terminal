use rstest::rstest;

use super::*;
use crate::control::{PaneInfo, ProjectInfo, Report, TabInfo, WorkspaceInfo};

fn running(program: &str, agent: Option<&str>, status: Option<&str>) -> Running {
    Running {
        program: program.into(),
        place: "shop › main".into(),
        agent: agent.map(str::to_string),
        status: status.map(str::to_string),
        resumes: false,
    }
}

fn report(panes: Vec<PaneInfo>) -> Report {
    let tab = TabInfo { panes, ..TabInfo::default() };
    let workspace = WorkspaceInfo { name: "main".into(), tabs: vec![tab], ..WorkspaceInfo::default() };
    let project = ProjectInfo { name: "shop".into(), workspaces: vec![workspace], ..ProjectInfo::default() };
    Report { projects: vec![project], ..Report::default() }
}

fn pane(program: &str, at_prompt: Option<bool>) -> PaneInfo {
    PaneInfo { program: Some(program.into()), at_prompt, ..PaneInfo::default() }
}

#[test]
fn nothing_running_says_so() {
    assert_eq!(
        confirmation(Some(&[])),
        "Nothing is running in your terminals. Your projects, workspaces, tabs and splits come back, \
             each tab with a new shell in its folder."
    );
}

#[test]
fn agents_are_counted_by_what_they_do_and_programs_are_named() {
    let all = [
        running("claude", Some("claude"), Some("working")),
        running("codex", Some("codex"), Some("working")),
        running("claude", Some("claude"), Some("waiting")),
        running("npm", None, None),
    ];
    assert_eq!(
        confirmation(Some(&all)),
        "Restarting stops every program running in cornercase's terminals. Running now: \
             3 agents (2 working, 1 waiting for you) and 1 other program (`npm` in shop › main). \
             Your projects, workspaces, tabs and splits come back, each tab with a new shell in its folder. \
             To pick up a Claude Code conversation afterwards, run `claude --continue` in its tab."
    );
}

#[rstest]
#[case::one(1, "1 agent conversation resumes in its tab.")]
#[case::two(2, "2 agent conversations resume in their tabs.")]
fn conversations_that_resume_are_counted_instead_of_the_hint(#[case] n: usize, #[case] said: &str) {
    let agents = [running("claude", Some("claude"), None), running("codex", Some("codex"), None)];
    let all: Vec<Running> =
        agents.into_iter().enumerate().map(|(i, agent)| Running { resumes: i < n, ..agent }).collect();
    let text = confirmation(Some(&all));
    assert_eq!((text.ends_with(said), text.contains("--continue")), (true, false));
}

#[rstest]
#[case::nothing(&[], None)]
#[case::one_program(&[("sleep", None)], Some("stopped 1 program"))]
#[case::agents_and_programs(
    &[("claude", Some("claude")), ("nvim", None), ("npm", None)],
    Some("stopped 1 agent and 2 other programs")
)]
fn stopped_counts_what_ran(#[case] ran: &[(&str, Option<&str>)], #[case] said: Option<&str>) {
    let ran: Vec<Running> = ran.iter().map(|&(program, agent)| running(program, agent, None)).collect();
    assert_eq!(stopped(&ran).as_deref(), said);
}

#[test]
fn a_pane_at_its_prompt_is_not_running() {
    let found = from_report(&report(vec![pane("zsh", Some(true)), pane("sleep", Some(false))]));
    assert_eq!(found, [running("sleep", None, None)]);
}

#[test]
fn an_agent_runs_even_where_its_pane_says_at_prompt() {
    let claude = PaneInfo { agent: Some("claude".into()), ..pane("claude", Some(true)) };
    assert_eq!(from_report(&report(vec![claude])), [running("claude", Some("claude"), None)]);
}

#[test]
fn the_pane_that_asks_is_not_listed() {
    let asking = PaneInfo { caller: true, ..pane("cornercase", Some(false)) };
    assert_eq!(from_report(&report(vec![asking, pane("nvim", Some(false))])), [running("nvim", None, None)]);
}

#[test]
fn an_old_server_s_shells_are_told_apart_by_name() {
    let found = from_report(&report(vec![pane("zsh", None), pane("nvim", None)]));
    assert_eq!(found, [running("nvim", None, None)]);
}

#[test]
fn stopping_the_server_says_what_stops_and_what_comes_back() {
    let all = [running("claude", Some("claude"), Some("working")), running("vite", None, None)];

    let text = stopping(Some(&all));

    assert!(text.starts_with("Stopping the server stops every program"), "{text}");
    assert!(text.contains("1 agent (1 working) and 1 other program (`vite` in shop › main)"), "{text}");
    assert!(text.ends_with("each tab with a new shell in its folder."), "{text}");
    assert!(!text.contains("resume"), "{text}");
}

#[test]
fn stopping_an_unreadable_server_names_no_programs() {
    assert!(stopping(None).starts_with("Stopping the server stops every program"));
}
