use ratatui::style::Color;

use super::*;

fn feed(w: &mut Watched, payload: &str) {
    w.app.projects[0].workspaces[0].tabs[0].panes[0].feed(format!("\x1b]7501;{payload}\x1b\\").as_bytes());
}

fn report(w: &mut Watched, state: &str, ticks: usize) {
    feed(w, &format!("state={state}"));
    for _ in 0..ticks {
        tick(&mut w.app);
    }
}

#[rstest]
#[case::claude(agents::Known::Claude)]
#[case::codex(agents::Known::Codex)]
#[case::gemini(agents::Known::Gemini)]
#[case::opencode(agents::Known::Opencode)]
#[case::grok(agents::Known::Grok)]
#[case::cursor(agents::Known::Cursor)]
#[case::copilot(agents::Known::Copilot)]
fn a_report_overrides_the_agents_heuristics_and_clear_returns_to_them(#[case] agent: agents::Known) {
    let mut w = Watched::start(agent, true);
    report(&mut w, "idle", 1);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Idle));
    assert!(w.told().1.is_empty(), "explicit idle is not a finish");
    report(&mut w, "blocked:kind=permission", 6);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Waiting));
    assert_eq!(w.told().1[0].text, format!("{} needs you in {}", agent.name(), w.place()));
    report(&mut w, "clear", 1);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Working));
}

#[test]
fn an_explicit_done_needs_no_working_transition_and_stays_acknowledged() {
    let mut w = Watched::start(agents::Known::Claude, true);
    report(&mut w, "idle", 1);
    report(&mut w, "done", 6);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Done));
    assert_eq!(w.told().1[0].text, format!("claude finished in {}", w.place()));
    click_row(&mut w.app, WorkspaceRow::Tab(0, 0));
    tick(&mut w.app);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Idle));
    w.app.add_tab(workspace_at(&w.app, 0, 0), AREA).expect("hide the finished tab");
    tick(&mut w.app);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Idle));
}

#[test]
fn failure_reaches_the_report_the_rollup_and_a_notification() {
    let mut w = Watched::start(agents::Known::Codex, true);
    report(&mut w, "error", 6);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Failed));
    assert_eq!(w.app.report(None).projects[0].workspaces[0].tabs[0].status.as_deref(), Some("failed"));
    assert_eq!(activity::attention([status(&w.app, 0, 0)]), Some(Status::Failed));
    assert_eq!(w.told().1[0].text, format!("codex failed in {}", w.place()));
    let row = row_rect(&w.app, WorkspaceRow::Tab(0, 0));
    let frame = render(&mut w.app, AREA);
    assert!((row.x..row.right()).any(|x| {
        let cell = &frame.backend().buffer()[(x, row.y)];
        cell.symbol() == "!" && cell.fg == Color::Red
    }));
    report(&mut w, "working", 1);
    assert_eq!(status(&w.app, 0, 0), Some(Status::Working));
}

#[test]
fn an_agent_without_a_reader_gets_its_status_from_the_protocol() {
    let (mut app, rx, _dirs) = app_with(1);
    let bin = TempDir::new();
    let amp = bin.path().join("amp");
    write_executable(&amp, "#!/bin/sh\nprintf '\\033]7501;state=working\\033\\\\'\nwhile read -r line; do :; done\n");
    type_line(&mut app, &amp.display().to_string());
    watch_until(&mut app, &rx, "Amp reports its status", |a| status(a, 0, 0) == Some(Status::Working));
    assert_eq!(app.projects[0].workspaces[0].tabs[0].panes[0].agent.agent(), Some("amp"));
    app.config.working_icon = "spinner".into();
    assert!(app.tick(Instant::now()).is_some_and(|wait| wait <= activity::SPIN_EVERY));
    let report = app.report(None);
    assert_eq!(report.projects[0].workspaces[0].tabs[0].panes[0].agent.as_deref(), Some("amp"));
}

#[test]
fn a_report_in_a_shell_does_not_turn_it_into_an_agent() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.term_mut().expect("shell").feed(b"\x1b]7501;state=blocked:app=cargo\x07");
    tick(&mut app);
    assert_eq!(status(&app, 0, 0), None);
}

#[rstest]
#[case::done("done", Status::Done)]
#[case::failure("error", Status::Failed)]
fn the_result_survives_the_agents_exit_and_does_not_override_the_next_agent(
    #[case] state: &str,
    #[case] expected: Status,
) {
    let mut w = Watched::start(agents::Known::Claude, true);
    report(&mut w, state, 1);
    let Fake::Claude(claude, _) = &w.agents[0] else { panic!("Claude fake") };
    claude.signal("quit");
    watch_until(&mut w.app, &w.rx, "the agent exits with its result kept", |a| {
        tab_term(a, 0, 0).shell_in_foreground() && status(a, 0, 0) == Some(expected)
    });
    assert_eq!(status(&w.app, 0, 0), Some(expected));
    click_row(&mut w.app, WorkspaceRow::Tab(0, 0));
    tick(&mut w.app);
    if expected == Status::Done {
        watch_until(&mut w.app, &w.rx, "the result is acknowledged", |a| status(a, 0, 0).is_none());
    }
    let fresh = FakeClaude::running(FakeClaude::SILENT);
    fresh.start(&mut w.app);
    watch_until(&mut w.app, &w.rx, "the next agent has its own state", |a| status(a, 0, 0) == Some(Status::Idle));
    fresh.signal("quit");
}

#[rstest]
#[case::idle("idle", None)]
#[case::finished("done", Some(Status::Done))]
fn a_shell_report_does_not_belong_to_the_departed_agent(#[case] state: &str, #[case] expected: Option<Status>) {
    let mut w = Watched::start(agents::Known::Claude, true);
    report(&mut w, state, 1);
    let Fake::Claude(claude, pid) = &w.agents[0] else { panic!("Claude fake") };
    let pid = *pid;
    claude.signal("quit");
    watch_until(&mut w.app, &w.rx, "the agent is gone", |a| {
        tab_term(a, 0, 0).shell_in_foreground() && !process::alive(pid)
    });
    report(&mut w, "error:id=build:app=cargo", 6);
    assert_eq!(status(&w.app, 0, 0), expected);
    assert!(w.told().1.iter().all(|notice| !notice.text.contains("claude failed")));
}

fn ticks(w: &mut Watched, count: usize) {
    for _ in 0..count {
        tick(&mut w.app);
    }
}

fn state_of(w: &Watched) -> Option<&'static str> {
    tab_term(&w.app, 0, 0).agent.state()
}

#[test]
fn a_turn_left_with_a_background_shell_reads_as_claude_reports_it() {
    let mut w = Watched::start(agents::Known::Claude, true);
    let mut states = Vec::new();

    for line in crate::test_util::fixture!("claude/2.1.325/background-shell.txt").lines() {
        let (file, payload) = line.split_once(' ').expect("a file status and a report");
        w.report(file, 0);
        if payload != "-" {
            feed(&mut w, payload);
        }
        ticks(&mut w, 1);
        states.push(state_of(&w));
    }
    ticks(&mut w, 5);

    let expected = ["working", "shell", "working", "working", "done"].map(Some);
    assert_eq!(states, expected);
    let told = w.told().1;
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].text, format!("claude finished in {}", w.place()));
}

#[rstest]
#[case::finished("done", Status::Done, Some("finished"))]
#[case::stopped("idle", Status::Idle, None)]
#[case::failed("error", Status::Failed, Some("failed"))]
fn a_reported_turn_over_waits_for_claudes_background_shell(
    #[case] state: &str,
    #[case] after: Status,
    #[case] notice: Option<&str>,
) {
    let mut w = Watched::start(agents::Known::Claude, true);
    report(&mut w, "working", 1);
    w.report("shell", 0);

    report(&mut w, &format!("{state}:app=claude-code"), 6);

    assert_eq!((status(&w.app, 0, 0), state_of(&w)), (Some(Status::Working), Some("shell")));
    assert!(w.told().1.is_empty());
    w.report("idle", 6);
    assert_eq!(status(&w.app, 0, 0), Some(after));
    let told = w.told().1;
    let said = told.first().map(|n| n.text.clone());
    assert_eq!(said, notice.map(|word| format!("claude {word} in {}", w.place())));
}

#[rstest]
#[case::the_file_catches_up(Some("shell"), "shell")]
#[case::the_file_never_does(None, "done")]
fn a_finish_reported_before_claudes_file_turns_waits_one_read(#[case] file: Option<&str>, #[case] expected: &str) {
    let mut w = Watched::start(agents::Known::Claude, true);
    report(&mut w, "working", 1);
    feed(&mut w, "state=done:app=claude-code");

    ticks(&mut w, 1);
    let held = state_of(&w);
    if let Some(file) = file {
        w.report(file, 0);
    }
    ticks(&mut w, 1);

    assert_eq!((held, state_of(&w)), (Some("working"), Some(expected)));
}
