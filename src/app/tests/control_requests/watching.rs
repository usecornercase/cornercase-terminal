use std::time::Duration;

use super::*;
use crate::control::Snapshot;
use crate::control::theme::ThemeFrom;
use crate::project::move_before;
use crate::project::split::Node;
use crate::test_util::git as run_git;

fn watch(app: &mut App) -> Snapshot {
    let answer = value(app, None, Command::Watch(wire::Watch::default()));
    serde_json::from_value(answer).expect("a snapshot")
}

fn watching(app: &mut App) -> Snapshot {
    app.clients.events.snapshots.set_every(Duration::ZERO);
    watch(app)
}

fn published(app: &mut App) -> Vec<Snapshot> {
    let parse = |text: &str| match serde_json::from_str(text) {
        Ok(Response::Ok(value)) => serde_json::from_value(value).expect("a snapshot"),
        other => panic!("not a snapshot: {other:?}"),
    };
    let take = app.take_events().into_iter();
    take.filter_map(|(_, streamed)| if let Streamed::Snapshot(text) = streamed { Some(parse(&text)) } else { None })
        .collect()
}

fn snapshots(app: &mut App) -> Vec<Snapshot> {
    lines(app).into_iter().map(|value| serde_json::from_value(value).expect("a snapshot")).collect()
}

fn settled(app: &mut App, rx: &Receiver<AppEvent>, from: u64) -> u64 {
    let mut revision = from;
    wait_until("every pane runs its shell and the session stops changing", || {
        drain(app, rx);
        let report = app.report(None);
        let mut panes = report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes);
        let shells = panes.all(|pane| {
            pane.program.as_deref().is_some_and(is_sh)
                && pane.at_prompt == Some(true)
                && !pane_screen(app, pane.id).trim().is_empty()
        });
        app.trace();
        let got = snapshots(app);
        revision = got.last().map_or(revision, |snapshot| snapshot.revision);
        shells && got.is_empty()
    });
    revision
}

fn next(app: &mut App, after: u64) -> Snapshot {
    let got = snapshots(app);
    let revisions: Vec<u64> = got.iter().map(|snapshot| snapshot.revision).collect();
    let expected: Vec<u64> = (after + 1..=after + got.len() as u64).collect();
    assert!(!got.is_empty(), "no snapshot after revision {after}");
    assert_eq!(revisions, expected, "every snapshot takes the next revision");
    got.into_iter().last().expect("a snapshot")
}

fn tab_named(snapshot: &Snapshot, tab: u64) -> Option<String> {
    snapshot
        .report
        .projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .flat_map(|w| &w.tabs)
        .find(|t| t.id == tab)?
        .custom_name
        .clone()
}

fn first_tab_id(app: &App) -> u64 {
    app.projects[0].workspaces[0].tabs[0].id
}

fn ratio(value: &Value) -> Option<f64> {
    match value {
        Value::Object(map) => map.get("ratio").and_then(Value::as_f64).or_else(|| map.values().find_map(ratio)),
        Value::Array(items) => items.iter().find_map(ratio),
        _ => None,
    }
}

fn rename(app: &mut App, item: Item, name: &str) {
    done(now(app, None, Command::Rename(wire::Rename { item: Some(item), name: name.into() })));
}

#[test]
fn the_answer_is_the_whole_session_with_its_revision_and_nothing_else_is_sent() {
    let (mut app, _rx, _dirs) = app_with(2);
    let tab = first_tab_id(&app);

    let snapshot = watching(&mut app);

    assert!(snapshot.revision >= 1, "{}", snapshot.revision);
    assert_eq!(snapshot.report.boot, wire::server_token());
    let projects: Vec<u64> = snapshot.report.projects.iter().map(|p| p.id).collect();
    assert_eq!(projects, app.projects.iter().map(|p| p.id).collect::<Vec<_>>());
    let tabs = &snapshot.report.projects[0].workspaces[0].tabs;
    assert_eq!((tabs[0].id, tabs[0].layout.is_some()), (tab, true));
    assert_eq!(app.take_events(), []);
}

#[test]
fn nothing_is_built_while_nobody_watches() {
    let (mut app, _rx, _dirs) = app_with(1);
    app.clients.events.snapshots.set_every(Duration::ZERO);

    for name in ["one", "two", "three"] {
        app.projects[0].name = Some(name.into());
        app.trace();
    }

    assert_eq!(app.take_events(), []);
    assert_eq!(watch(&mut app).revision, 1);
}

#[test]
fn a_session_that_did_not_change_sends_nothing_and_keeps_its_revision() {
    let (mut app, rx, _dirs) = app_with(1);
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);

    app.trace();
    app.trace();

    assert_eq!(snapshots(&mut app), []);
    app.projects[0].collapsed = true;
    assert_eq!(next(&mut app, revision).revision, revision + 1);
}

#[test]
fn renaming_a_tab_or_a_project_sends_the_whole_session_with_the_next_revision() {
    let (mut app, rx, _dirs) = app_with(2);
    let tab = first_tab_id(&app);
    let other = app.projects[1].id;
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);

    rename(&mut app, Item::Tab(tab), "build");
    let renamed = next(&mut app, revision);
    rename(&mut app, Item::Project(other), "api");
    let again = next(&mut app, renamed.revision);

    assert_eq!(tab_named(&renamed, tab).as_deref(), Some("build"));
    assert_eq!(renamed.report.projects.len(), 2);
    let project = again.report.projects.iter().find(|p| p.id == other).expect("the project");
    assert_eq!((project.name.as_str(), project.custom_name.as_deref()), ("api", Some("api")));
    assert_eq!(tab_named(&again, tab).as_deref(), Some("build"));
}

#[test]
fn moving_a_project_sends_the_new_order() {
    let (mut app, rx, _dirs) = app_with(2);
    let ids: Vec<u64> = app.projects.iter().map(|p| p.id).collect();
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);

    move_before(&mut app.projects, 1, 0, Some(&mut app.active));

    let order: Vec<u64> = next(&mut app, revision).report.projects.iter().map(|p| p.id).collect();
    assert_eq!(order, [ids[1], ids[0]]);
}

#[test]
fn a_split_and_its_divider_send_the_new_layout() {
    let (mut app, rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);

    let split = wire::Split { pane: Some(pane), down: false, command: None, focus: false };
    let added = done(now(&mut app, None, Command::Split(split))).ids.pane.expect("the new pane");
    let snapshot = next(&mut app, revision);
    let tab = &snapshot.report.projects[0].workspaces[0].tabs[0];
    let mut panes: Vec<u64> = tab.panes.iter().map(|p| p.id).collect();
    panes.sort_unstable();
    assert_eq!(panes, [pane, added]);
    assert!(tab.layout.is_some());
    let revision = settled(&mut app, &rx, snapshot.revision);

    let Node::Split { ratio: dragged, .. } = &mut app.projects[0].workspaces[0].tabs[0].layout else {
        panic!("the tab is split")
    };
    *dragged = 0.25;
    let line = lines(&mut app).pop().expect("a snapshot");

    let got = ratio(&line).expect("a ratio");
    assert!((got - 0.25).abs() < 1e-6, "{got}");
    assert_eq!(serde_json::from_value::<Snapshot>(line).expect("a snapshot").revision, revision + 1);
}

#[test]
fn folding_and_a_group_style_send_the_new_session() {
    let (mut app, rx, _dirs) = app_with(1);
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);

    app.projects[0].collapsed = true;
    let folded = next(&mut app, revision);
    let group = app.add_group("work".into());
    let grouped = next(&mut app, folded.revision);
    app.groups[0].entry.icon = '◆';
    app.groups[0].entry.colour = 33;
    let styled = next(&mut app, grouped.revision);

    assert!(folded.report.projects[0].collapsed);
    assert_eq!(grouped.report.groups.iter().map(|g| g.id).collect::<Vec<_>>(), [group]);
    let style = &styled.report.groups[0];
    assert_eq!((style.icon.as_str(), style.colour), ("◆", 33));
}

#[test]
fn an_arranged_session_shows_in_the_next_snapshot() {
    let (mut app, rx, _dirs) = app_with(2);
    let projects: Vec<u64> = app.projects.iter().map(|p| p.id).collect();
    let pane = term(&app, 0).id;
    let revision = watching(&mut app).revision;
    let mut revision = settled(&mut app, &rx, revision);

    let group = done(Response::Ok(ok(&mut app, "group-new", &json!({"name": "work"})))).group.expect("a group");
    let snapshot = next(&mut app, revision);
    assert_eq!(snapshot.report.groups.iter().map(|g| g.id).collect::<Vec<_>>(), [group]);
    revision = snapshot.revision;

    ok(&mut app, "move", &json!({"item": {"project": projects[1]}, "into": group}));
    let snapshot = next(&mut app, revision);
    let moved = snapshot.report.projects.iter().find(|p| p.id == projects[1]).expect("the project");
    assert_eq!(moved.group, Some(group));
    revision = snapshot.revision;

    ok(&mut app, "collapse", &json!({"item": {"group": group}}));
    let snapshot = next(&mut app, revision);
    assert!(snapshot.report.groups[0].collapsed);
    revision = snapshot.revision;

    let added = done(Response::Ok(ok(&mut app, "split", &json!({"pane": pane})))).ids.pane.expect("a pane");
    revision = next(&mut app, revision).revision;
    revision = settled(&mut app, &rx, revision);
    ok(&mut app, "move-pane", &json!({"pane": added, "target": pane, "place": "left"}));
    let snapshot = next(&mut app, revision);
    let tab = &snapshot.report.projects[0].workspaces[0].tabs[0];
    let active: Vec<u64> = tab.panes.iter().filter(|p| p.active).map(|p| p.id).collect();
    assert_eq!(active, [added]);
    assert_eq!(serde_json::to_value(&tab.layout).expect("json")["split"]["first"]["leaf"], added);
}

#[test]
fn changes_within_the_throttle_wait_and_go_out_as_one_snapshot() {
    let (mut app, rx, _dirs) = app_with(1);
    let tab = first_tab_id(&app);
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);
    let every = Duration::from_secs(3600);
    app.clients.events.snapshots.set_every(every);
    assert_eq!(app.tick(Instant::now()), None, "nothing waits before a step");

    rename(&mut app, Item::Tab(tab), "first");
    app.trace();
    rename(&mut app, Item::Tab(tab), "second");
    app.trace();
    let waited = app.take_events();
    let wake = app.tick(Instant::now()).expect("a wake-up for the snapshot");
    app.clients.events.snapshots.set_every(Duration::ZERO);
    let snapshot = next(&mut app, revision);

    assert_eq!(waited, []);
    assert!(wake <= every, "{wake:?}");
    assert_eq!((snapshot.revision, tab_named(&snapshot, tab).as_deref()), (revision + 1, Some("second")));
}

#[test]
fn output_alone_publishes_nothing_until_a_step_is_traced() {
    let (mut app, rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let revision = watching(&mut app).revision;
    settled(&mut app, &rx, revision);

    let wrote = app.projects[0].workspaces[0].tabs[0].panes[0].write(b"echo watched-\"\"output\n");
    refreshing(&mut app, &rx, "the shell prints", |app| pane_screen(app, pane).contains("watched-output"));

    assert!(wrote);
    assert_eq!(app.take_events(), []);
}

#[test]
fn output_alone_does_not_build_the_session_again_even_when_the_refresh_reads_the_panes() {
    let (mut app, rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let revision = watching(&mut app).revision;
    settled(&mut app, &rx, revision);
    app.agents.set_watched(Some(Instant::now()));
    app.trace_refresh();
    app.take_events();
    let built = app.clients.events.snapshots.built();

    let wrote = app.projects[0].workspaces[0].tabs[0].panes[0].write(b"echo cells-\"\"only\n");
    wait_until("the shell prints", || {
        drain(&mut app, &rx);
        pane_screen(&app, pane).contains("cells-only")
    });
    app.trace_refresh();
    let after_output = (app.clients.events.snapshots.built(), app.take_events());
    app.agents.set_watched(Some(Instant::now()));
    app.trace_refresh();

    assert!(wrote);
    assert_eq!(after_output, (built, Vec::new()));
    assert_eq!((app.clients.events.snapshots.built(), app.take_events()), (built, Vec::new()));
}

#[test]
fn watching_again_after_leaving_gets_the_current_session_with_a_later_revision() {
    let (mut app, rx, _dirs) = app_with(1);
    let tab = first_tab_id(&app);
    let first = watching(&mut app);
    let revision = settled(&mut app, &rx, first.revision);

    app.forget(CLIENT);
    rename(&mut app, Item::Tab(tab), "later");
    assert_eq!(snapshots(&mut app), []);
    let again = watch(&mut app);

    assert_eq!(again.report.boot, first.report.boot);
    assert!(again.revision > revision, "{} after {revision}", again.revision);
    assert_eq!(tab_named(&again, tab).as_deref(), Some("later"));
}

#[test]
fn a_branch_checked_out_outside_cornercase_sends_the_new_branch() {
    let repo = git_repo(&[("README", "hi")]);
    let (mut app, rx) = empty_app();
    app.open_project(repo.path().to_path_buf(), AREA).expect("open the repository");
    let revision = watching(&mut app).revision;
    settled(&mut app, &rx, revision);
    app.refresh(Instant::now());
    app.trace_refresh();
    app.take_events();
    app.refresh(Instant::now() + WATCH_AGENTS_EVERY);
    app.trace_refresh();
    let quiet = app.take_events();

    run_git(repo.path(), &["checkout", "--quiet", "-b", "feat/outside"]);
    app.refresh(Instant::now() + WATCH_AGENTS_EVERY * 2);
    app.trace_refresh();

    let branches: Vec<Option<String>> =
        published(&mut app).iter().map(|snapshot| snapshot.report.projects[0].workspaces[0].branch.clone()).collect();
    assert_eq!(quiet, []);
    assert_eq!(branches, [Some("feat/outside".to_string())]);
}

#[test]
fn a_title_the_program_sets_reaches_status_and_the_next_snapshot() {
    let (mut app, rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let revision = watching(&mut app).revision;
    settled(&mut app, &rx, revision);
    app.refresh(Instant::now());
    app.trace_refresh();
    let before = app.report(None).projects[0].workspaces[0].tabs[0].panes[0].title.clone();
    published(&mut app);

    app.output(pane, b"]2;watched-title");
    app.refresh(Instant::now() + WATCH_AGENTS_EVERY);
    app.trace_refresh();

    let titles: Vec<Option<String>> =
        published(&mut app).iter().map(|s| s.report.projects[0].workspaces[0].tabs[0].panes[0].title.clone()).collect();
    assert_eq!(before, None);
    assert_eq!(titles, [Some("watched-title".to_string())]);
}

#[test]
fn a_theme_change_sends_the_whole_session_with_the_next_revision() {
    let (mut app, _rx, _dirs) = app_with(1);
    let first = watching(&mut app);
    assert_eq!(first.report.theme.map(|t| t.from), Some(ThemeFrom::Default));

    let theme = serde_json::to_value(crate::test_util::theme(libghostty_vt::style::RgbColor { r: 1, g: 2, b: 3 }))
        .expect("json");
    let text = serde_json::json!({"command": "theme", "args": {"theme": theme}}).to_string();
    app.request(9, &text, Some(AREA), Instant::now());
    app.take_answers();
    app.trace();
    app.refresh(Instant::now() + Duration::from_secs(1));

    let snapshot = next(&mut app, first.revision);
    assert_eq!(snapshot.report.theme.map(|t| t.from), Some(ThemeFrom::Client));
}

#[test]
fn a_fold_of_an_agents_children_reaches_watchers() {
    let (mut app, rx, _dirs) = app_with(1);
    let revision = watching(&mut app).revision;
    let revision = settled(&mut app, &rx, revision);
    click_row(&mut app, WorkspaceRow::NewTab(0));
    let ids: Vec<u64> = (0..2).map(|t| tab_term(&app, 0, t).id).collect();
    for tab in &mut app.projects[0].workspaces[0].tabs {
        let term = tab.pane_mut().expect("a pane");
        term.agent.follow(Some("claude"));
        term.agent.update(Some(activity::Activity::Idle), true, Instant::now());
    }
    app.projects[0].workspaces[0].tabs[1].pane_mut().expect("the child").parent = Some(ids[0]);
    render(&mut app, AREA);
    app.jump_to_pane(ids[0]);
    render(&mut app, AREA);
    app.trace();
    let revision = snapshots(&mut app).last().map_or(revision, |snapshot| snapshot.revision);
    let collapsed = |snapshot: &Snapshot| {
        let panes =
            snapshot.report.projects.iter().flat_map(|p| &p.workspaces).flat_map(|w| &w.tabs).flat_map(|t| &t.panes);
        panes.filter(|pane| pane.collapsed).map(|pane| pane.id).collect::<Vec<_>>()
    };

    ok(&mut app, "collapse", &json!({"item": {"pane": ids[0]}}));
    let folded = next(&mut app, revision);
    ok(&mut app, "focus", &json!({"item": {"pane": ids[1]}}));
    render(&mut app, AREA);
    app.trace();
    let opened = next(&mut app, folded.revision);

    assert_eq!((collapsed(&folded), collapsed(&opened)), (vec![ids[0]], Vec::<u64>::new()));
}
