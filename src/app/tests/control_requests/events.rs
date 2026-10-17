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
    let close =
        wire::Close { item: Item::Tab(ids.tab.expect("a tab")), remove_worktree: false, force: false, unlock: false };
    done(now(&mut app, None, Command::Close(close)));
    let gone = until_told(&mut app, &rx, "the tab closes", |streamed| told(streamed).len() >= 2);

    let tab = wire::Ids { pane: None, ..ids };
    assert_eq!(
        opened,
        [
            (wire::What::Opened { kind: wire::Kind::Tab, parent: None }, tab),
            (wire::What::Opened { kind: wire::Kind::Pane, parent: None }, ids),
        ]
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
        let close = wire::Close { item: Item::Pane(pane), remove_worktree: false, force: false, unlock: false };
        done(now(&mut app, None, Command::Close(close)));
    }
    let got = until_told(&mut app, &rx, "the followed pane closes", |streamed| streamed.last() == Some(&Streamed::End));

    assert_eq!(told(&got), [(wire::What::Closed { kind: wire::Kind::Pane }, ids)]);
    assert!(!app.events.listening());
}

#[test]
fn a_pane_that_does_not_exist_cannot_be_followed() {
    let (mut app, _rx, _dirs) = app_with(1);

    let message =
        error(now(&mut app, None, Command::Events(wire::Events { panes: vec![999], host: false, notices: false })));

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

fn host_told(app: &mut App) -> Vec<(wire::What, wire::Ids)> {
    let streamed = app.take_events();
    told(&streamed.into_iter().map(|(_, streamed)| streamed).collect::<Vec<_>>())
}

#[test]
fn a_programs_clipboard_write_and_bell_reach_a_follower_that_asked() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let ids = first_tab(&app, pane);
    follow_with(&mut app, &[], true);
    app.take_host_writes();

    app.output(pane, b"\x1b]52;c;aGVsbG8=\x07\x07");

    assert_eq!(
        host_told(&mut app),
        [(wire::What::Clipboard { text: "hello".into() }, ids), (wire::What::Bell { count: 1 }, ids)]
    );
    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello")]);
}

#[test]
fn a_follower_that_did_not_ask_gets_neither() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    follow(&mut app, &[]);
    app.take_host_writes();

    app.output(pane, b"\x1b]52;c;aGVsbG8=\x07\x07");

    assert_eq!(host_told(&mut app), []);
    assert_eq!(app.take_host_writes(), [clipboard::osc52("hello")]);
}

#[rstest::rstest]
#[case::a_read(b"\x1b]52;c;?\x07")]
#[case::the_primary_selection(b"\x1b]52;p;aGVsbG8=\x07")]
#[case::a_title_ended_by_bel(b"\x1b]0;title\x07")]
fn what_is_not_a_clipboard_write_or_a_bell_tells_nothing(#[case] output: &[u8]) {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    follow_with(&mut app, &[], true);

    app.output(pane, output);

    assert_eq!(host_told(&mut app), []);
}

#[test]
fn host_events_of_other_panes_are_left_out() {
    let (mut app, _rx, _dirs) = app_with(1);
    let followed = term(&app, 0).id;
    let ids = first_tab(&app, followed);
    let other = done(now(&mut app, None, new_tab(None))).ids.pane.expect("a pane");
    follow_with(&mut app, &[followed], true);
    app.take_events();

    app.output(other, b"\x1b]52;c;aGVsbG8=\x07");
    let left_out = host_told(&mut app);
    app.output(followed, b"\x1b]52;c;aGVsbG8=\x07");

    assert_eq!(left_out, []);
    assert_eq!(host_told(&mut app), [(wire::What::Clipboard { text: "hello".into() }, ids)]);
}

#[test]
fn bells_ringing_in_a_loop_are_told_at_most_once_a_second_with_their_count() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let ids = first_tab(&app, pane);
    follow_with(&mut app, &[], true);
    for _ in 0..4 {
        app.output(pane, b"\x07");
    }
    let start = Instant::now();
    let first = host_told(&mut app);
    let wake = app.tick(start);
    app.refresh(start + crate::app::events::BELL_EVERY);
    let second = host_told(&mut app);
    app.refresh(start + crate::app::events::BELL_EVERY * 3);

    assert_eq!(first, [(wire::What::Bell { count: 1 }, ids)]);
    assert!(wake.is_some_and(|wake| wake <= crate::app::events::BELL_EVERY));
    assert_eq!(second, [(wire::What::Bell { count: 3 }, ids)]);
    assert_eq!(host_told(&mut app), []);
}

#[test]
fn bells_before_anyone_asked_are_not_told_later() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;

    app.output(pane, b"\x07\x07");
    follow_with(&mut app, &[], true);
    app.refresh(Instant::now() + crate::app::events::BELL_EVERY * 2);

    assert_eq!(host_told(&mut app), []);
}

#[test]
fn notices_reach_only_the_readers_that_asked_and_a_pane_reader_gets_those_naming_its_pane() {
    let (mut app, _rx, _dirs) = app_with(1);
    let pane = term(&app, 0).id;
    let other = done(now(&mut app, None, new_tab(None))).ids.pane.expect("a pane");
    app.events.subscribe(CLIENT, vec![pane], false, true);
    app.events.subscribe(CLIENT + 1, Vec::new(), false, false);
    let (project, workspace) = (app.projects[0].id, app.projects[0].workspaces[0].id);
    let _ = app.take_events();

    app.notify("claude", activity::Status::Waiting, (project, workspace), vec![other]);
    app.notify("claude", activity::Status::Waiting, (project, workspace), vec![pane]);

    let tokens: Vec<_> = app
        .take_events()
        .into_iter()
        .filter_map(|(client, streamed)| match streamed {
            Streamed::Event(text) if text.contains("\"event\":\"notice\"") => Some((client, text)),
            _ => None,
        })
        .collect();
    assert_eq!(tokens.len(), 1, "{tokens:?}");
    assert_eq!(tokens[0].0, CLIENT);
    assert!(tokens[0].1.contains("\"panes\":[") && tokens[0].1.contains(&format!("{pane}")), "{tokens:?}");
}
