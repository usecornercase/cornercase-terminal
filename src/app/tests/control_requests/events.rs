use super::*;

fn first_tab(app: &App, pane: u64) -> wire::Ids {
    let workspace = &app.projects[0].workspaces[0];
    let (project, workspace, tab) = (app.projects[0].id, workspace.id, workspace.tabs[0].id);
    wire::Ids { project: Some(project), workspace: Some(workspace), tab: Some(tab), pane: Some(pane) }
}

#[test]
fn tabs_and_panes_are_told_as_they_open_and_close_with_their_ids() {
    let (mut app, rx, _dirs) = app_with(1);
    follow(&mut app, &[]);

    let ids = done(now(&mut app, None, new_tab(None))).ids;
    let opened = told(&streamed(&mut app));
    let close = wire::Close { item: Item::Tab(ids.tab.expect("a tab")), remove_worktree: false, force: false };
    done(now(&mut app, None, Command::Close(close)));
    let gone = until_told(&mut app, &rx, "the tab closes", |streamed| told(streamed).len() >= 2);

    let tab = wire::Ids { pane: None, ..ids };
    assert_eq!(
        opened,
        [(wire::What::Opened { kind: wire::Kind::Tab }, tab), (wire::What::Opened { kind: wire::Kind::Pane }, ids)]
    );
    assert_eq!(
        told(&gone),
        [(wire::What::Closed { kind: wire::Kind::Pane }, ids), (wire::What::Closed { kind: wire::Kind::Tab }, tab)]
    );
}

#[test]
fn following_panes_tells_only_about_them_and_ends_once_they_all_closed() {
    let (mut app, rx, _dirs) = app_with(1);
    let followed = term(&app, 0).id;
    let ids = first_tab(&app, followed);
    let other = done(now(&mut app, None, new_tab(None))).ids.pane.expect("a pane");
    follow(&mut app, &[followed]);

    done(now(&mut app, None, new_tab(None)));
    for pane in [other, followed] {
        let close = wire::Close { item: Item::Pane(pane), remove_worktree: false, force: false };
        done(now(&mut app, None, Command::Close(close)));
    }
    let got = until_told(&mut app, &rx, "the followed pane closes", |streamed| streamed.last() == Some(&Streamed::End));

    assert_eq!(told(&got), [(wire::What::Closed { kind: wire::Kind::Pane }, ids)]);
    assert!(!app.events.listening());
}

#[test]
fn a_pane_that_does_not_exist_cannot_be_followed() {
    let (mut app, _rx, _dirs) = app_with(1);

    let message = error(now(&mut app, None, Command::Events(wire::Events { panes: vec![999] })));

    assert!(message.contains("there is no pane 999"), "{message}");
    assert!(!app.events.listening());
}

#[test]
fn a_client_that_left_is_told_nothing_more() {
    let (mut app, _rx, _dirs) = app_with(1);
    follow(&mut app, &[]);

    app.forget(CLIENT);
    done(now(&mut app, None, new_tab(None)));

    assert_eq!(streamed(&mut app), []);
}

#[test]
fn a_program_that_ends_is_told_by_its_name() {
    let (mut app, rx, _dirs) = app_with(1);
    follow(&mut app, &[]);
    let pane = term(&app, 0).id;
    let typed = Launch::command(pane, "sleep 30".into(), Instant::now());
    app.agents.launches.push(typed);
    refreshing(&mut app, &rx, "sleep runs", |app| pane_job(app, pane).is_some_and(|job| job.program == "sleep"));

    done(now(&mut app, None, press_keys(pane, &["ctrl+c"])));
    let got = until_told(&mut app, &rx, "sleep ends", |streamed| {
        told(streamed).iter().any(|(what, _)| matches!(what, wire::What::Exited { .. }))
    });

    let ids = first_tab(&app, pane);
    assert!(told(&got).contains(&(wire::What::Exited { program: "sleep".into() }, ids)), "{:?}", told(&got));
}

#[test]
fn programs_are_not_looked_up_while_nobody_follows() {
    let (mut app, rx, _dirs) = app_with(1);
    let id = term(&app, 0).id;
    app.agents.launches.push(Launch::command(id, "sleep 30".into(), Instant::now()));
    refreshing(&mut app, &rx, "sleep runs", |app| pane(app, id).program(&app.config).as_deref() == Some("sleep"));

    app.agents.watched = None;
    app.refresh(Instant::now());

    assert_eq!(pane_job(&app, id), None);
}

fn pane_job(app: &App, id: u64) -> Option<crate::term::Job> {
    pane(app, id).job.clone()
}
