use std::time::Duration;

use serde_json::json;

use super::*;
use crate::agents::activity::{Activity, Status};
use crate::app::tests::flow_panel::team as lightweight;
use crate::control::{FlowBacklog, FlowMessage, FlowRuns, RunEdge, RunFiles, RunInfo, Snapshot};
use crate::flows::progress::Progress;

type Fed = Snapshot<FlowRuns>;
type Shown<'a> = (u64, &'a str, &'a str, &'a str, Option<&'a str>, usize, Option<RunEdge>);

fn follow(app: &mut App) -> Fed {
    app.clients.events.runs.set_every(Duration::ZERO);
    let answer = value(app, None, Command::FlowRuns(wire::FlowRunsWatch::default()));
    serde_json::from_value(answer).expect("a runs snapshot")
}

fn fed(app: &mut App) -> Vec<Fed> {
    lines(app).into_iter().map(|value| serde_json::from_value(value).expect("a runs snapshot")).collect()
}

fn last(app: &mut App) -> Fed {
    fed(app).pop().expect("a snapshot")
}

fn role<'a>(fed: &'a Fed, name: &str) -> &'a wire::RoleState {
    fed.report.runs[0].roles.iter().find(|role| role.role == name).expect("the role")
}

fn pane_of(app: &App, role: &str) -> u64 {
    app.runs.all()[0].pane_of(role).expect("a member")
}

fn tell(app: &mut App, from: &str, to: &str, text: &str) {
    let send = wire::SendText { pane: Some(pane_of(app, to)), text: Some(text.into()), ..wire::SendText::default() };
    ask(app, Some(pane_of(app, from)), Command::Send(send));
    assert_eq!(answers(app).len(), 1);
}

fn events(app: &mut App) -> Vec<FlowMessage> {
    streamed(app)
        .into_iter()
        .filter_map(|streamed| match streamed {
            Streamed::Event(text) => match serde_json::from_str(&text) {
                Ok(Response::Ok(value)) => Some(serde_json::from_value(value).expect("a message")),
                other => panic!("not a message: {other:?}"),
            },
            Streamed::Snapshot(_) | Streamed::End => None,
        })
        .collect()
}

fn backlog(app: &mut App) -> FlowBacklog {
    serde_json::from_value(ok(app, "flow-messages", &json!({}))).expect("a backlog")
}

fn assert_run_begun(app: &mut App, started: &RunInfo) {
    let begun = last(app);
    let run = &begun.report.runs[0];
    assert_eq!((run.run, run.workspace, run.project), (started.run, started.workspace, started.project));
    assert_eq!((run.stage, run.holder.clone(), run.last.clone(), run.ended), (None, None, None, false));
    assert_eq!(run.flow, wire::FlowSpec::from(&pair()));
    assert_eq!((run.task.as_str(), run.dir.clone()), ("Fix the login form", started.run_dir.clone()));
    let roles: Vec<(&str, Option<u64>)> = run.roles.iter().map(|r| (r.role.as_str(), r.pane)).collect();
    let members: Vec<(&str, Option<u64>)> = started.members.iter().map(|m| (m.role.as_str(), Some(m.pane))).collect();
    assert_eq!(roles, members);
    assert!(run.roles.iter().all(|r| r.tab.is_some() && !r.closed));
}

fn send_around(team: &mut Team, started: &RunInfo) {
    let orchestrator = started.members[0].pane;
    let (coder, reviewer) = (started.members[1].pane, started.members[2].pane);
    for (to, pane) in [("coder", coder), ("reviewer", reviewer), ("coder", coder)] {
        let send = wire::SendText { pane: Some(pane), text: Some(format!("go {to}")), ..wire::SendText::default() };
        ask(&mut team.app, Some(orchestrator), Command::Send(send));
        answered(&mut team.app, &team.rx, "the send");
    }
}

fn assert_run_stood(stood: &Fed) {
    let run = &stood.report.runs[0];
    assert_eq!(
        (run.stage, run.holder.as_deref(), run.rounds.clone(), run.messages),
        (Some(0), Some("coder"), vec![0, 1], 3)
    );
    let step = run.last.clone().expect("the last step");
    assert_eq!(
        (step.edge, step.from.as_deref(), step.to.as_str()),
        (RunEdge::Loop { stage: 1 }, Some("reviewer"), "coder")
    );
}

#[test]
fn a_run_with_the_fake_claude_is_followed_from_its_start_to_its_end() {
    let mut team = Team::new();
    let first = follow(&mut team.app);
    assert_eq!(first.report.runs, Vec::new());
    assert_eq!(first.report.boot, wire::server_token());

    let started = team.started();
    team.until_got(3);
    assert_run_begun(&mut team.app, &started);
    send_around(&mut team, &started);

    let stood = last(&mut team.app);
    assert_run_stood(&stood);

    let tabs: Vec<u64> = stood.report.runs[0].roles.iter().filter_map(|r| r.tab).collect();
    for tab in tabs {
        ok(&mut team.app, "close", &json!({"item": {"tab": tab}}));
    }
    refreshing(&mut team.app, &team.rx, "the run ends", |app| app.runs.all().is_empty());
    let ended = last(&mut team.app);
    assert_eq!(ended.report.runs, Vec::new());
    assert!(ended.revision > stood.revision);
}

#[test]
fn the_first_snapshot_of_a_server_without_runs_is_empty_and_pacing_sends_nothing_unchanged() {
    let (mut app, _rx, _dir) = lightweight();
    let first = follow(&mut app);
    assert_eq!(first.report.runs.len(), 1);

    assert_eq!(fed(&mut app), []);
    assert_eq!(fed(&mut app), []);
}

#[test]
fn the_last_step_outside_the_plan_and_the_end_are_told() {
    let (mut app, _rx, _dir) = lightweight();
    let first = follow(&mut app);
    assert_eq!((first.report.runs[0].stage, first.report.runs[0].outside), (None, None));
    tell(&mut app, "orchestrator", "coder", "go");

    tell(&mut app, "orchestrator", "tester", "skip ahead");

    let run = &last(&mut app).report.runs[0];
    assert_eq!(run.outside, Some(RunEdge::Outside { from: Some(0), to: 2 }));
    assert_eq!(run.last.as_ref().map(|step| step.edge), run.outside);
    assert_eq!((run.stage, run.holder.as_deref(), run.ended), (Some(2), Some("tester"), false));

    let orchestrator = pane_of(&app, "orchestrator");
    let term = app.projects.pane_by_mut(orchestrator).expect("its pane");
    term.agent.follow(Some("claude"));
    term.agent.update(Some(Activity::Idle), true, Instant::now());
    term.agent.mark(true);
    let marked = last(&mut app);
    assert!(marked.report.runs[0].ended);
    assert_eq!(role(&marked, "orchestrator").status.as_deref(), Some(Status::Pending.name()));
}

#[test]
fn a_member_whose_tab_closed_is_closed_and_the_rest_are_not() {
    let (mut app, rx, _dir) = lightweight();
    let first = follow(&mut app);
    let tab = role(&first, "tester").tab.expect("its tab");
    let pane = pane_of(&app, "tester");

    ok(&mut app, "close", &json!({"item": {"tab": tab}}));
    refreshing(&mut app, &rx, "the pane goes", |app| app.projects.pane_by(pane).is_none());

    let after = last(&mut app);
    let tester = role(&after, "tester");
    assert_eq!((tester.closed, tester.pane, tester.tab), (true, Some(pane), None));
    assert!(!role(&after, "coder").closed);
}

#[test]
fn a_run_restored_with_its_stage_and_rounds_has_no_messages_and_a_new_id() {
    let (mut app, _rx, _dir) = lightweight();
    let flow = app.runs.all()[0].flow.clone();
    app.runs.first_mut().progress = Progress::resumed(&flow, Some(1), &[0, 2, 0]);
    app.runs.first_mut().id += 100;

    let run = &follow(&mut app).report.runs[0];

    assert_eq!(
        (run.run, run.stage, run.rounds.clone(), run.messages),
        (app.runs.all()[0].id, Some(1), vec![0, 2, 0], 0)
    );
    assert_eq!(run.last, None);
    assert_eq!(backlog(&mut app).messages.len(), 0);
}

#[test]
fn the_messages_of_a_run_come_with_what_was_sent_before() {
    let (mut app, _rx, _dir) = lightweight();
    tell(&mut app, "orchestrator", "coder", "Write it.");
    tell(&mut app, "orchestrator", "reviewer", "Review it.");

    let kept = backlog(&mut app);

    assert_eq!(
        (kept.boot.as_str(), kept.run, kept.workspace),
        (wire::server_token(), app.runs.all()[0].id, app.runs.all()[0].workspace)
    );
    let shown: Vec<Shown> = kept
        .messages
        .iter()
        .map(|m| (m.seq, m.from.as_str(), m.to.as_str(), m.kind.as_str(), m.text.as_deref(), m.bytes, m.edge))
        .collect();
    assert_eq!(
        shown,
        [
            (1, "orchestrator", "coder", "send", Some("Write it."), 9, None),
            (2, "orchestrator", "reviewer", "send", Some("Review it."), 10, Some(RunEdge::Forward { stage: 0 })),
        ]
    );
    assert_eq!(events(&mut app), []);

    tell(&mut app, "orchestrator", "coder", "Fix it.");
    let read = wire::Read { pane: Some(pane_of(&app, "coder")), ..wire::Read::default() };
    let asker = pane_of(&app, "orchestrator");
    ask(&mut app, Some(asker), Command::Read(read));
    answers(&mut app);

    let live = events(&mut app);
    let told: Vec<(u64, &str, &str)> = live.iter().map(|m| (m.seq, m.kind.as_str(), m.to.as_str())).collect();
    assert_eq!(told, [(3, "send", "coder"), (4, "read", "coder")]);
    assert_eq!(live[0].edge, Some(RunEdge::Loop { stage: 1 }));
    assert_eq!((live[1].text.clone(), live[0].run), (None, app.runs.all()[0].id));
}

#[test]
fn a_long_text_keeps_its_real_size() {
    let (mut app, _rx, _dir) = lightweight();
    backlog(&mut app);

    tell(&mut app, "orchestrator", "coder", &"x".repeat(40 * 1024));

    let message = events(&mut app).remove(0);
    assert_eq!((message.bytes, message.text.map(|text| text.len())), (40 * 1024, Some(32 * 1024)));
}

#[test]
fn a_messages_stream_ends_when_its_run_does() {
    let (mut app, _rx, _dir) = lightweight();
    backlog(&mut app);

    app.runs.clear();
    app.forget_runs();

    let ended: Vec<Streamed> = streamed(&mut app);
    assert!(ended.contains(&Streamed::End), "{ended:?}");
    assert!(!app.clients.events.follows_flow_messages(CLIENT));
}

#[test]
fn a_second_messages_stream_on_one_connection_is_a_conflict() {
    let (mut app, _rx, _dir) = lightweight();
    backlog(&mut app);

    let refused = refused(&mut app, "flow-messages", &json!({}));

    assert_eq!(refused.code, "conflict");
}

#[test]
fn a_workspace_without_a_run_has_no_messages_and_an_unknown_one_is_not_found() {
    let (mut app, _rx, _dirs) = app_with(1);

    let none = refused(&mut app, "flow-messages", &json!({}));
    let unknown = refused(&mut app, "flow-messages", &json!({"workspace": 99999}));

    assert_eq!(none.code, "conflict");
    assert!(none.message.contains("flow runs"), "{}", none.message);
    assert_eq!(unknown.code, "not-found");
}

mod files {
    use super::*;

    fn folder(app: &mut App, names: &[&str]) -> TempDir {
        let dir = TempDir::new();
        for (age, name) in names.iter().enumerate() {
            let path = dir.path().join(name);
            std::fs::write(&path, name).expect("a file");
            let at = SystemTime::now() - Duration::from_secs(100 * (age as u64 + 1));
            std::fs::File::options().write(true).open(&path).expect("open").set_modified(at).expect("a time");
        }
        app.runs.first_mut().dir = Some(dir.path().to_path_buf());
        dir
    }

    fn listed(app: &mut App, rx: &Receiver<AppEvent>) -> RunFiles {
        serde_json::from_value(asked(app, rx, "flow-files", &json!({})).expect("a listing")).expect("files")
    }

    #[test]
    fn the_run_folder_is_listed_newest_first() {
        let (mut app, rx, _dir) = lightweight();
        let dir = folder(&mut app, &["newest.md", "older.md", "oldest.md"]);

        let files = listed(&mut app, &rx);

        let names: Vec<&str> = files.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["newest.md", "older.md", "oldest.md"]);
        assert_eq!(
            (files.run, files.workspace, files.more),
            (Some(app.runs.all()[0].id), app.runs.all()[0].workspace, false)
        );
        assert_eq!(files.dir, dir.path());
        assert_eq!(files.files[0].path, dir.path().join("newest.md"));
        assert_eq!(files.files[0].bytes, 9);
        assert!(files.files[0].modified.as_deref().is_some_and(|at| at.ends_with('Z')));
    }

    #[test]
    fn more_than_the_panel_shows_is_said() {
        let (mut app, rx, _dir) = lightweight();
        let names: Vec<String> = (0..201).map(|n| format!("f{n:03}.md")).collect();
        let _kept = folder(&mut app, &names.iter().map(String::as_str).collect::<Vec<_>>());

        let files = listed(&mut app, &rx);

        assert_eq!((files.files.len(), files.more), (200, true));
    }

    #[test]
    fn the_last_folder_is_listed_after_the_run_ended() {
        let (mut app, rx, _dir) = lightweight();
        let dir = folder(&mut app, &["answer.md"]);
        let workspace = app.runs.all()[0].workspace;
        let (p, w) = app.projects.workspace_index(workspace).expect("the workspace");
        app.projects[p].workspaces[w].run_dirs.push(dir.path().to_path_buf());
        app.runs.clear();

        let files = listed(&mut app, &rx);

        assert_eq!((files.run, files.files.len()), (None, 1));
    }

    #[test]
    fn a_workspace_that_never_ran_a_flow_is_a_conflict() {
        let (mut app, rx, _dirs) = app_with(1);

        let refused = asked(&mut app, &rx, "flow-files", &json!({})).expect_err("refused");

        assert_eq!(refused.code, "conflict");
        assert!(refused.message.contains("no run folder"), "{}", refused.message);
    }

    #[test]
    fn a_folder_that_is_gone_fails_with_its_path() {
        let (mut app, rx, _dir) = lightweight();
        let dir = folder(&mut app, &["answer.md"]);
        let gone = dir.path().to_path_buf();
        drop(dir);

        let refused = asked(&mut app, &rx, "flow-files", &json!({})).expect_err("refused");

        assert_eq!(refused.code, "failed");
        assert!(refused.message.contains(&gone.display().to_string()), "{}", refused.message);
    }

    #[test]
    fn an_unknown_workspace_is_not_found() {
        let (mut app, rx, _dir) = lightweight();

        let refused = asked(&mut app, &rx, "flow-files", &json!({"workspace": 99999})).expect_err("refused");

        assert_eq!(refused.code, "not-found");
    }
}
