use super::*;
use crate::control::PaneInfo;

fn listing(streaming: &mut Streaming, panes: &[u64]) {
    let mut report = worked(report(&[("web-shop", Some(SHOP)), ("blog", None)]));
    let pane = |&id: &u64| PaneInfo { id, program: Some("zsh".into()), active: id == 1, ..PaneInfo::default() };
    report.projects[0].workspaces[0].tabs[1].panes = panes.iter().map(pane).collect();
    serve(&mut streaming.seeing.theirs, 2, report);
    streaming.seeing.rig.until("the panes are listed", |app| {
        let seen = app.machines.held()[0].seen.as_ref();
        seen.is_some_and(|seen| seen.projects[0].workspaces[0].tabs[1].panes.len() == panes.len())
    });
}

fn split_there() -> Streaming {
    split_listing(&[1, 2])
}

fn split_listing(panes: &[u64]) -> Streaming {
    let mut streaming = streaming();
    listing(&mut streaming, panes);
    say(&mut streaming.surface, &THERE.split(1, 1, "none"));
    streaming.seeing.rig.until("the split arrives", |app| {
        app.watched().and_then(|watched| watched.screen.as_deref()).is_some_and(|screen| screen.panes.len() == 2)
    });
    streaming
}

fn by_keys(app: &mut App, codes: &[KeyCode]) {
    app.config.prefix_key = "ctrl+]".into();
    send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    for code in codes {
        send_key(app, *code, KeyModifiers::NONE);
    }
}

fn asked(streaming: &Streaming) -> (UnixStream, Value) {
    let mut line = reach(&streaming.seeing.ends);
    let asked = request(&mut line);
    (line, asked)
}

fn opened(app: &App) -> Option<&'static str> {
    match &app.overlay {
        Some(Overlay::MachineWorkspace(dialog)) => Some(dialog.name()),
        _ => None,
    }
}

fn still_there(app: &App) -> bool {
    app.shown_remote().is_some_and(|shown| shown.workspace == 10)
}

#[rstest]
#[case::right('|', false)]
#[case::down('-', true)]
fn a_split_splits_the_active_pane_there_and_focuses_the_new_one(#[case] key: char, #[case] down: bool) {
    let mut streaming = split_there();

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char(key)]);
    let (mut line, split) = asked(&streaming);
    say(&mut line, &json!({"ok": {"workspace": 10, "tab": 13, "pane": 3}}));
    let ends = Arc::clone(&streaming.seeing.ends);
    streaming.seeing.rig.until("the focus is ordered", |_| !ends.lock().is_empty());
    let (_, focus) = asked(&streaming);

    assert_eq!((&split["command"], &split["boot"]), (&json!("split"), &json!(BOOT)));
    assert_eq!((&split["args"]["pane"], &split["args"]["down"]), (&json!(1), &json!(down)));
    assert_eq!((&focus["command"], &focus["view"]), (&json!("focus"), &json!(VIEW)));
    assert_eq!(focus["args"]["item"], serde_json::to_value(Item::Pane(3)).expect("an item"));
    assert!(still_there(&streaming.seeing.rig.app));
}

#[test]
fn a_move_focuses_the_pane_on_that_side_and_nothing_where_none_is() {
    let mut streaming = split_there();

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Left]);
    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Right]);

    let (_, focus) = asked(&streaming);
    assert_eq!(focus["args"]["item"], serde_json::to_value(Item::Pane(2)).expect("an item"));
    assert!(still_there(&streaming.seeing.rig.app));
}

#[test]
fn closing_a_pane_asks_then_closes_the_active_one_there() {
    let mut streaming = split_there();

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('x')]);
    assert_eq!(opened(&streaming.seeing.rig.app), Some("close machine pane"));
    send_key(&mut streaming.seeing.rig.app, KeyCode::Enter, KeyModifiers::NONE);

    let (_, close) = asked(&streaming);
    assert_eq!((&close["command"], &close["args"]["item"]), (&json!("close"), &json!({"pane": 1})));
    assert!(still_there(&streaming.seeing.rig.app));
}

#[test]
fn closing_the_only_pane_asks_to_close_its_tab() {
    let mut streaming = streaming();
    listing(&mut streaming, &[1]);
    live(&mut streaming, "hello");

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('x')]);

    assert_eq!(opened(&streaming.seeing.rig.app), Some("close machine tab"));
}

#[rstest]
#[case::split(&[KeyCode::Char('|')])]
#[case::close_pane(&[KeyCode::Char('x')])]
#[case::move_right(&[KeyCode::Right])]
fn before_the_first_frame_sends_nothing_and_says_so(#[case] codes: &[KeyCode]) {
    let mut streaming = streaming();

    by_keys(&mut streaming.seeing.rig.app, codes);

    assert_eq!(toast(&streaming.seeing.rig.app), Some("vps unreachable: nothing was sent"));
    assert_eq!((opened(&streaming.seeing.rig.app), still_there(&streaming.seeing.rig.app)), (None, true));
}

#[rstest]
#[case::rename_tab(&[KeyCode::Char('r')], "rename machine tab")]
#[case::rename_workspace(&[KeyCode::Char('w'), KeyCode::Char('r')], "rename machine workspace")]
#[case::close_workspace(&[KeyCode::Char('w'), KeyCode::Char('x')], "close machine workspace")]
fn a_rename_or_a_close_asks_about_what_is_there(#[case] codes: &[KeyCode], #[case] dialog: &str) {
    let mut streaming = split_there();

    by_keys(&mut streaming.seeing.rig.app, codes);

    assert_eq!(opened(&streaming.seeing.rig.app), Some(dialog));
    assert!(still_there(&streaming.seeing.rig.app));
}

#[test]
fn a_new_tab_opens_one_in_the_workspace_there() {
    let mut streaming = split_there();

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('c')]);

    let (_, new) = asked(&streaming);
    assert_eq!((&new["command"], &new["args"]["workspace"]), (&json!("new-tab"), &json!(10)));
    assert!(still_there(&streaming.seeing.rig.app));
}

#[test]
fn closing_a_pane_the_window_lists_alone_still_closes_only_the_pane_while_the_frame_shows_two() {
    let mut streaming = split_listing(&[1]);

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('x')]);

    assert_eq!(opened(&streaming.seeing.rig.app), Some("close machine pane"));
}

#[test]
fn closing_a_pane_the_machine_has_not_listed_yet_sends_nothing_and_says_so() {
    let mut streaming = split_listing(&[2]);

    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('x')]);

    assert_eq!(opened(&streaming.seeing.rig.app), None);
    assert_eq!(toast(&streaming.seeing.rig.app), Some("vps has not listed that pane yet; nothing was sent"));
}

#[test]
fn a_split_answered_after_another_tab_was_picked_does_not_pull_the_view_back() {
    let mut streaming = split_there();
    by_keys(&mut streaming.seeing.rig.app, &[KeyCode::Char('|')]);
    let (mut line, _) = asked(&streaming);

    assert!(streaming.seeing.rig.app.select_remote("vps", RemoteItem::Tab(11)));
    let mut orders = reach(&streaming.seeing.ends);
    assert_eq!(request(&mut orders)["args"]["item"], serde_json::to_value(Item::Tab(11)).expect("an item"));
    say(&mut orders, &json!({"ok": {"tab": 11}}));
    let mut frame = There { tab: 11, ..THERE }.complete(2, 1, "other");
    frame["ok"]["panes"][0]["pane"] = json!(12);
    frame["ok"]["layout"] = json!({"leaf": 12});
    say(&mut streaming.surface, &frame);
    streaming.seeing.rig.until("the other tab shows", |app| {
        app.watched().and_then(|watched| watched.screen.as_deref()).is_some_and(|screen| screen.tab == 11)
    });
    say(&mut line, &json!({"ok": {"workspace": 10, "tab": 13, "pane": 3}}));
    streaming.seeing.rig.until("the split's answer is heard", |app| app.machines.acting.waiting() == 0);
    let pane = streaming.seeing.rig.app.layout(AREA).pane;
    mouse(&mut streaming.seeing.rig.app, MouseEventKind::ScrollUp, Position::new(pane.x + 3, pane.y + 1));

    assert_eq!(request(&mut orders)["command"], "scroll");
}
