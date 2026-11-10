use serde_json::{Value, json};

use super::*;
use crate::app::tests::flow_panel::team as lightweight;
use crate::control::{CloseWhenMergedSet, FlowRuns};
use crate::flows::End;

fn listed(app: &App) -> FlowRuns {
    app.flow_runs_state()
}

fn toggle(app: &mut App, workspace: u64, off: bool) -> CloseWhenMergedSet {
    serde_json::from_value(ok(app, "flow-close-when-merged", &json!({"workspace": workspace, "off": off})))
        .expect("an answer")
}

fn workspace(app: &App) -> u64 {
    app.runs.all()[0].workspace
}

#[test]
fn a_run_started_with_close_when_merged_is_opted_in_and_says_so() {
    let mut team = Team::new();
    let run = wire::FlowRun {
        name: "Pair".into(),
        prompt: "Fix the login form".into(),
        close_when_merged: true,
        ..wire::FlowRun::default()
    };
    ask(&mut team.app, None, Command::FlowRun(run));

    let Response::Ok(value) = answered(&mut team.app, &team.rx, "the flow's tabs are open") else { panic!("refused") };

    let info: RunInfo = serde_json::from_value(value).expect("the run");
    assert!(info.close_when_merged);
    assert!(team.app.runs.all()[0].close_when_merged);
    assert!(listed(&team.app).runs[0].close_when_merged);
}

#[test]
fn close_when_merged_on_a_flow_that_leaves_the_commits_is_refused_and_opens_nothing() {
    let mut team = Team::new();
    let flows = paths::flows(&team.config.path().join("config.json"));
    store::save(&flows, &Flow { name: "Quick".into(), end: End::Commits, ..pair() }, None).expect("save");
    let tabs = team.app.projects[0].workspaces[0].tabs.len();
    let workspaces = team.app.projects[0].workspaces.len();
    let args = json!({"name": "Quick", "prompt": "x", "close_when_merged": true});

    let refused = refused(&mut team.app, "flow-run", &args);

    assert!(refused.message.contains("leaves the commits"), "{}", refused.message);
    assert!(team.app.runs.all().is_empty());
    assert_eq!(team.app.projects[0].workspaces.len(), workspaces);
    assert_eq!(team.app.projects[0].workspaces[0].tabs.len(), tabs);
}

#[test]
fn the_request_sets_and_clears_it_and_says_whether_it_changed() {
    let (mut app, _rx, _dir) = lightweight();
    let id = workspace(&app);

    let on = toggle(&mut app, id, false);
    assert_eq!((on.close_when_merged, on.changed, on.workspace), (true, true, id));
    assert!(listed(&app).runs[0].close_when_merged);
    assert!(!toggle(&mut app, id, false).changed);

    let off = toggle(&mut app, id, true);
    assert_eq!((off.close_when_merged, off.changed), (false, true));
    assert!(!listed(&app).runs[0].close_when_merged);
    assert!(!toggle(&mut app, id, true).changed);
}

fn commits(app: &mut App) -> (Value, &'static str) {
    app.runs.first_mut().flow.end = End::Commits;
    (json!({"workspace": workspace(app), "off": false}), "leaves the commits")
}

fn kept(app: &mut App) -> (Value, &'static str) {
    let id = workspace(app);
    let plan = app.agents_close_plan(id).expect("a plan");
    app.close_agents(&plan);
    (json!({"workspace": id, "off": false}), "reopen them first")
}

fn unknown(_: &mut App) -> (Value, &'static str) {
    (json!({"workspace": 9999, "off": false}), "workspace")
}

fn no_run(app: &mut App) -> (Value, &'static str) {
    let id = app.projects[0].workspaces[0].id;
    app.runs.take_of(id);
    (json!({"workspace": id, "off": false}), "runs no flow")
}

#[rstest::rstest]
#[case::unknown_workspace(unknown, "not-found")]
#[case::no_run(no_run, "conflict")]
#[case::kept_run(kept, "conflict")]
#[case::commits_flow(commits, "conflict")]
fn the_request_is_refused_where_there_is_no_live_run(
    #[case] setup: fn(&mut App) -> (Value, &'static str),
    #[case] code: &str,
) {
    let (mut app, _rx, _dir) = lightweight();
    let (args, says) = setup(&mut app);

    let refused = refused(&mut app, "flow-close-when-merged", &args);

    assert_eq!(refused.code, code);
    assert!(refused.message.contains(says), "{}", refused.message);
}
