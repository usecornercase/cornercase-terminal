use super::*;

#[test]
fn status_lists_every_pane_and_marks_the_caller() {
    let (mut app, _rx, _dirs) = app_with(2);
    let (caller, other) = (term(&app, 0).id, term(&app, 1).id);

    let report: Report =
        serde_json::from_value(value(&mut app, Some(caller), Command::Status(wire::Status {}))).expect("a report");

    let panes: Vec<(u64, bool)> = report
        .projects
        .iter()
        .flat_map(|p| &p.workspaces)
        .flat_map(|w| &w.tabs)
        .flat_map(|t| &t.panes)
        .map(|p| (p.id, p.caller))
        .collect();
    assert_eq!(panes, [(caller, true), (other, false)]);
    assert_eq!((report.caller, report.shown.pane), (Some(caller), Some(other)));
}

#[test]
fn a_pane_of_another_server_is_not_the_caller() {
    let (mut app, _rx, _dirs) = app_with(1);
    let caller = Some(term(&app, 0).id);
    let status = Command::Status(wire::Status {});
    let text = serde_json::to_string(&wire::Request { caller, server: Some("another".into()), command: status })
        .expect("json");

    app.request(CLIENT, &text, Some(AREA), Instant::now());

    let report: Report = serde_json::from_value(match answers(&mut app).remove(0) {
        Response::Ok(value) => value,
        Response::Error(message) => panic!("{message}"),
    })
    .expect("a report");
    assert_eq!(report.caller, None);
}

#[test]
fn a_project_being_closed_is_opened_again() {
    let (mut app, _rx, dirs) = app_with(1);
    let old = app.projects[0].id;
    app.close_project(old);
    let open = Command::Open(wire::Open { path: dirs[0].path().to_path_buf(), focus: false });

    let opened = done(now(&mut app, None, open));

    assert!(opened.ids.project.is_some_and(|id| id != old), "{opened:?}");
}

#[test]
fn open_adds_a_folder_once_and_leaves_the_window_as_it_is() {
    let (mut app, _rx, _dirs) = app_with(1);
    let dir = TempDir::new();
    let open = || Command::Open(wire::Open { path: dir.path().to_path_buf(), focus: false });

    let opened = done(now(&mut app, None, open()));
    let found = done(now(&mut app, None, open()));

    assert_eq!((app.projects.len(), app.active), (2, 0));
    assert_eq!((opened.ids.project, found.ids.project), (Some(app.projects[1].id), Some(app.projects[1].id)));
}

#[test]
fn a_new_tab_leaves_the_window_as_it_is() {
    let (mut app, _rx, _dirs) = app_with(2);
    let before = app.focus();
    let workspace = app.projects[0].workspaces[0].id;
    let named = wire::NewTab { workspace: Some(workspace), name: Some("tests".into()), ..wire::NewTab::default() };

    let made = done(now(&mut app, None, Command::NewTab(named)));

    let tabs = &app.projects[0].workspaces[0].tabs;
    assert_eq!(app.focus(), before);
    assert_eq!((tabs.len(), tabs[1].name.as_deref()), (2, Some("tests")));
    assert_eq!(made.ids.pane, tabs[1].pane().map(|t| t.id));
}

#[test]
fn a_command_is_typed_into_the_new_tab_before_the_answer() {
    let (mut app, rx) = app();

    ask(&mut app, None, new_tab(Some("echo typed-$((40+2))")));
    assert_eq!(answers(&mut app), []);
    let made = done(answered(&mut app, &rx, "the command is typed"));

    let id = made.ids.pane.expect("the new pane");
    pump_until(&mut app, &rx, "the command runs", |a| pane_screen(a, id).contains("typed-42"));
}

#[test]
fn focus_shows_what_it_names() {
    let (mut app, _rx, _dirs) = app_with(2);
    let id = term(&app, 0).id;

    done(now(&mut app, None, Command::Focus(wire::Focus { item: Item::Pane(id) })));

    assert_eq!((app.active, app.term().map(|t| t.id)), (0, Some(id)));
}

#[test]
fn a_split_keeps_the_focus_where_it_was() {
    let (mut app, _rx) = app();
    let was = first(&app);

    let made = done(now(&mut app, None, Command::Split(wire::Split { down: true, ..wire::Split::default() })));

    let tab = app.tab().expect("a tab");
    assert_eq!((tab.panes.len(), tab.pane().map(|t| t.id)), (2, Some(was)));
    assert_eq!(made.ids.pane, Some(tab.panes[1].id));
}

#[test]
fn new_terminals_need_a_window_to_take_their_size_from() {
    let (mut app, _rx) = app();

    request(&mut app, None, new_tab(None), None);

    assert!(error(answers(&mut app).remove(0)).contains("has not opened a window yet"));
}

#[test]
fn an_unknown_command_names_the_version_of_the_server() {
    let (mut app, _rx) = app();

    app.request(CLIENT, r#"{"command": "teleport", "args": {}}"#, Some(AREA), Instant::now());

    let message = error(answers(&mut app).remove(0));
    assert!(message.contains("has no `teleport` command") && message.contains(version::CURRENT), "{message}");
}
