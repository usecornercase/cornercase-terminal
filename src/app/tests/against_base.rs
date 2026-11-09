#![cfg(unix)]

use super::*;
use crate::control::{self as wire, Command};
use crate::control::{OpenView, Report, SetView, ViewInfo};
use crate::project::Hierarchy;
use crate::test_util::git;
use std::time::Duration;

fn topic_repo() -> TempDir {
    let repo = git_repo(&[("a.txt", "one\ntwo\n")]);
    git(repo.path(), &["checkout", "--quiet", "-b", "topic"]);
    std::fs::write(repo.path().join("b.txt"), "bee\n").expect("write");
    git(repo.path(), &["add", "--all"]);
    git(repo.path(), &["commit", "--quiet", "-m", "b"]);
    std::fs::write(repo.path().join("a.txt"), "one\nTWO\n").expect("edit");
    repo
}

fn two_projects() -> (App, Receiver<AppEvent>, Vec<TempDir>) {
    let dirs = vec![repo_with_edit(), topic_repo()];
    let (mut app, rx) = empty_app();
    for dir in &dirs {
        app.open_project(dir.path().to_path_buf(), AREA).expect("open project");
    }
    (app, rx, dirs)
}

fn workspace_ids(app: &App) -> Vec<u64> {
    app.projects.iter().filter_map(|p| p.workspace().map(|w| w.id)).collect()
}

fn report(app: &App) -> Report {
    app.report(None)
}

fn against(app: &App, workspace: u64) -> Option<wire::AgainstBase> {
    let report = report(app);
    let all = report.projects.iter().flat_map(|p| p.workspaces.iter());
    all.into_iter().find(|w| w.id == workspace).and_then(|w| w.against_base.clone())
}

fn changes_of(app: &App, workspace: u64) -> Option<wire::ChangesCount> {
    let report = report(app);
    let all = report.projects.iter().flat_map(|p| p.workspaces.iter());
    all.into_iter().find(|w| w.id == workspace).and_then(|w| w.changes)
}

fn call(app: &mut App, client: u64, view: Option<u64>, command: Command) -> serde_json::Value {
    let request = wire::Request { caller: None, server: None, contract: None, boot: None, view, command };
    app.request(client, &serde_json::to_string(&request).expect("json"), Some(AREA), Instant::now());
    let answers = app.take_answers();
    let (_, text) = answers.into_iter().find(|(who, _)| *who == client).expect("an answer");
    serde_json::from_str(&text).expect("json")
}

fn open_view_on(app: &mut App, tab: u64) -> u64 {
    let open = OpenView { cols: Some(80), rows: Some(24), tab: Some(tab), ..OpenView::default() };
    let answer = call(app, 71, None, Command::View(open));
    let info: ViewInfo = serde_json::from_value(answer["ok"]["view"].clone()).expect("a view");
    info.id
}

fn row_text(app: &mut App) -> String {
    format!("{}", render(app, AREA).backend())
}

#[test]
fn without_a_window_nothing_runs() {
    let repo = topic_repo();
    let (mut app, rx) = app_in(repo.path(), no_config());
    let workspace = workspace_ids(&app)[0];
    for _ in 0..5 {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
    }
    assert!(!app.panels.changes.tallying());
    assert_eq!(against(&app, workspace), None);
}

#[test]
fn a_window_counts_the_active_project_only() {
    let (mut app, rx, _dirs) = two_projects();
    app.window_attached();
    let ids = workspace_ids(&app);
    let (shown, other) = (ids[app.active], ids[1 - app.active]);
    refreshing(&mut app, &rx, "the count arrives", |a| against(a, shown).is_some());
    let count = against(&app, shown).expect("counted");
    assert_eq!((count.base.as_str(), count.files, count.added, count.removed), ("main", 2, 2, 1));
    assert_eq!(against(&app, other), None);
    assert!(row_text(&mut app).contains("+2 \u{2212}1"));
}

#[test]
fn a_changed_base_hides_the_count_until_it_is_read_again() {
    let (mut app, rx, _dirs) = two_projects();
    app.window_attached();
    let shown = workspace_ids(&app)[app.active];
    refreshing(&mut app, &rx, "the count arrives", |a| against(a, shown).is_some());
    app.projects.workspace_by_mut(shown).expect("workspace").base = Some("topic".into());
    assert_eq!(against(&app, shown), None);
    refreshing(&mut app, &rx, "the count is read again", |a| against(a, shown).is_some());
    assert_eq!(against(&app, shown).map(|c| c.files), Some(1));
}

#[test]
fn a_view_makes_every_workspace_counted_and_counts_the_one_it_shows() {
    let (mut app, rx, _dirs) = two_projects();
    app.window_attached();
    let ids = workspace_ids(&app);
    let other = ids[1 - app.active];
    let tab = app
        .projects
        .iter()
        .filter(|p| p.workspace().is_some_and(|w| w.id == other))
        .find_map(|p| p.workspace().and_then(|w| w.tab()).map(|t| t.id));
    let view = open_view_on(&mut app, tab.expect("a tab"));
    refreshing(&mut app, &rx, "every workspace is counted", |a| ids.iter().all(|id| against(a, *id).is_some()));
    refreshing(&mut app, &rx, "the viewed workspace has its changes", |a| changes_of(a, other).is_some());
    let count = changes_of(&app, other).expect("counted");
    assert_eq!((count.mode, count.files), (changes::Mode::Uncommitted, 1));
    let hidden = SetView { visible: Some(false), ..SetView::default() };
    call(&mut app, 71, Some(view), Command::ViewSet(hidden));
    assert_eq!(against(&app, other), None);
}

#[test]
fn closing_a_workspace_forgets_its_count() {
    let (mut app, rx, _dirs) = two_projects();
    app.window_attached();
    let shown = workspace_ids(&app)[app.active];
    refreshing(&mut app, &rx, "the count arrives", |a| against(a, shown).is_some());
    app.panels.changes.forget(shown);
    assert!(app.panels.changes.tally(shown, None).is_none());
}

#[test]
fn a_count_that_lands_pushes_a_new_watch_line() {
    let (mut app, rx, _dirs) = two_projects();
    app.clients.events.snapshots.set_every(Duration::ZERO);
    call(&mut app, 72, None, Command::Watch(wire::Watch::default()));
    app.window_attached();
    let shown = workspace_ids(&app)[app.active];
    let mut pushed = false;
    wait_until("a pushed line carries the count", || {
        drain(&mut app, &rx);
        app.refresh(Instant::now());
        app.trace();
        for (_, streamed) in app.take_events() {
            if let Streamed::Snapshot(text) = streamed {
                pushed |= text.contains("against_base");
            }
        }
        pushed
    });
    assert!(against(&app, shown).is_some());
}
