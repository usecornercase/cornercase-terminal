use rstest::rstest;

use super::*;
use crate::app::Streamed;
use crate::app::surfaces::FRAME_EVERY;
use crate::control::contract::Code;
use crate::control::surface::{Frame, Surface};
use crate::control::{OpenView, SetView, ViewInfo};

const A: u64 = 31;
const B: u64 = 32;

fn call(app: &mut App, client: u64, view: Option<u64>, command: Command, structured: bool) -> Value {
    let contract = structured.then_some(1);
    let request = wire::Request { caller: None, server: None, contract, boot: None, view, command };
    app.request(client, &serde_json::to_string(&request).expect("json"), Some(AREA), Instant::now());
    let answers = app.take_answers();
    let (who, text) = answers.into_iter().find(|(who, _)| *who == client).expect("an answer");
    assert_eq!(who, client);
    serde_json::from_str(&text).expect("json")
}

fn ok(mut value: Value) -> Value {
    match value.get_mut("ok").map(Value::take) {
        Some(answer) => answer,
        None => panic!("the request failed: {value}"),
    }
}

fn code(value: &Value) -> Value {
    value["error"]["code"].clone()
}

fn open(app: &mut App, client: u64, size: Option<(u16, u16)>, tab: Option<u64>) -> ViewInfo {
    let open = OpenView { cols: size.map(|s| s.0), rows: size.map(|s| s.1), tab, ..OpenView::default() };
    let answer = ok(call(app, client, None, Command::View(open), false));
    assert_eq!(answer["revision"], 1);
    serde_json::from_value(answer["view"].clone()).expect("a view")
}

fn set(app: &mut App, view: u64, client: u64, set: SetView) -> Value {
    call(app, client, Some(view), Command::ViewSet(set), true)
}

fn focus(app: &mut App, view: u64, client: u64, tab: u64) -> Value {
    call(app, client, Some(view), Command::Focus(wire::Focus { item: Item::Tab(tab), reveal: None }), false)
}

fn info(app: &mut App, view: u64) -> ViewInfo {
    let report = ok(call(app, 70, Some(view), Command::Status(wire::Status {}), false));
    serde_json::from_value(report["view"].clone()).expect("a view")
}

fn report(app: &mut App) -> Report {
    serde_json::from_value(ok(call(app, 70, None, Command::Status(wire::Status {}), false))).expect("a report")
}

fn tabs(app: &mut App, n: usize) -> Vec<u64> {
    for _ in 1..n {
        done(Response::Ok(ok(call(app, 71, None, new_tab(None), false))));
    }
    app.projects[0].workspaces[0].tabs.iter().map(|t| t.id).collect()
}

fn size_of(app: &App, tab: u64) -> (u16, u16) {
    let tab = app.projects.tabs().find(|t| t.id == tab).expect("the tab");
    let (rows, cols) = tab.pane().expect("a pane").emulator.size().expect("a size");
    (cols, rows)
}

fn window_pane(app: &App) -> (u16, u16) {
    let pane = app.layout(AREA).pane;
    (pane.width, pane.height)
}

fn snapshots(app: &mut App, client: u64) -> Vec<Value> {
    app.clients.events.snapshots.set_every(Duration::ZERO);
    app.trace();
    app.take_events()
        .into_iter()
        .filter(|(who, _)| *who == client)
        .filter_map(|(_, streamed)| match streamed {
            Streamed::Snapshot(text) => Some(ok(serde_json::from_str(&text).expect("json"))),
            Streamed::Event(_) | Streamed::End => None,
        })
        .collect()
}

fn surface(app: &mut App, client: u64, view: u64) -> Value {
    let command = Command::Surface(Surface::default());
    call(app, client, Some(view), command, true)
}

fn frames_for(app: &mut App, client: u64, at: Instant) -> Vec<Frame> {
    app.render_surfaces(&[client], at)
        .into_iter()
        .filter_map(|(_, surfaced)| match surfaced {
            Surfaced::Frame(text) => {
                Some(serde_json::from_value(ok(serde_json::from_str(&text).expect("json"))).expect("a frame"))
            }
            Surfaced::End => None,
        })
        .collect()
}

#[test]
fn a_view_opens_on_what_the_window_shows_and_moves_nothing() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 2);
    let (shown, active) = (report(&mut app).shown, app.projects[0].workspaces[0].active);

    let view = open(&mut app, A, None, None);

    assert_eq!((view.shown, view.projection, view.active, view.controls), (shown, 1, true, false));
    assert_eq!((view.cols, view.rows), (None, None));
    assert_eq!(report(&mut app).shown, shown);
    assert_eq!((app.projects[0].workspaces[0].active, ids.len()), (active, 2));
}

#[test]
fn two_clients_view_different_tabs_and_neither_moves_the_window() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 3);
    let window = report(&mut app).shown;
    let (a, b) = (open(&mut app, A, Some((100, 30)), Some(ids[0])), open(&mut app, B, Some((80, 20)), Some(ids[1])));
    app.resize(AREA);

    let moved = ok(focus(&mut app, a.id, A, ids[2]));

    assert_eq!(moved["tab"], ids[2]);
    assert_eq!(moved["projection"], 2);
    assert_eq!(info(&mut app, a.id).shown.tab, Some(ids[2]));
    assert_eq!(info(&mut app, b.id).shown.tab, Some(ids[1]));
    assert_eq!(report(&mut app).shown, window);
    assert_eq!(size_of(&app, ids[2]), (100, 30));
    assert_eq!(size_of(&app, ids[1]), (80, 20));
    assert_eq!(size_of(&app, ids[0]), window_pane(&app));
}

#[test]
fn a_new_tab_in_a_view_lands_in_its_workspace_and_moves_only_the_view() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 2);
    let window = report(&mut app).shown;
    let view = open(&mut app, A, Some((90, 25)), Some(ids[1]));

    let created = ok(call(
        &mut app,
        A,
        Some(view.id),
        Command::NewTab(wire::NewTab { focus: true, ..wire::NewTab::default() }),
        false,
    ));

    let tab = created["tab"].as_u64().expect("a tab");
    assert_eq!(created["projection"], 2);
    assert_eq!(info(&mut app, view.id).shown.tab, Some(tab));
    assert_eq!(report(&mut app).shown, window);
    assert_eq!(size_of(&app, tab), (90, 25));
}

#[test]
fn a_views_size_has_no_window_chrome_and_other_tabs_keep_the_window_size() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 3);

    open(&mut app, A, Some((100, 30)), Some(ids[1]));
    app.resize(AREA);

    assert_eq!(size_of(&app, ids[1]), (100, 30));
    assert_eq!(size_of(&app, ids[2]), window_pane(&app));
    assert_eq!(size_of(&app, ids[0]), window_pane(&app));
}

#[test]
fn a_tab_sized_by_a_view_has_no_account_row_while_the_window_s_tabs_do() {
    let (mut app, _rx, _dirs) = app_with(1);
    let root = TempDir::new();
    app.set_accounts_dir(root.path());
    crate::accounts::add(root.path(), crate::agents::Known::Claude, "work", None).expect("add the account");
    app.relist_accounts();
    let ids = tabs(&mut app, 2);

    open(&mut app, A, Some((80, 20)), Some(ids[1]));
    app.resize(AREA);

    let (cols, rows) = window_pane(&app);
    assert_eq!((size_of(&app, ids[1]), size_of(&app, ids[0])), ((80, 20), (cols, rows - 1)));
}

#[test]
fn two_sizes_on_one_tab_go_to_the_latest_claim_and_never_bounce() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 2)[1];
    let a = open(&mut app, A, Some((100, 30)), Some(tab));
    let b = open(&mut app, B, Some((80, 20)), Some(tab));

    for _ in 0..3 {
        app.resize(AREA);
        app.render_surfaces(&[A, B], Instant::now());
        assert_eq!(size_of(&app, tab), (80, 20));
    }

    let controller = report(&mut app).projects[0].workspaces[0].tabs[1].controller;
    assert_eq!(controller, Some(b.id));
    assert!(!info(&mut app, a.id).controls);
    assert!(info(&mut app, b.id).controls);
}

#[test]
fn control_passes_to_the_next_claim_and_back_to_the_window() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 2)[1];
    let a = open(&mut app, A, Some((100, 30)), Some(tab));
    let b = open(&mut app, B, Some((80, 20)), Some(tab));

    set(&mut app, b.id, B, SetView { active: Some(false), ..SetView::default() });
    app.resize(AREA);
    assert_eq!(size_of(&app, tab), (100, 30));
    let refused = set(&mut app, b.id, B, SetView { claim: true, ..SetView::default() });
    assert_eq!(code(&refused), "failed");
    set(&mut app, b.id, B, SetView { active: Some(true), ..SetView::default() });
    assert_eq!(size_of(&app, tab), (80, 20));

    app.forget(B);
    app.resize(AREA);
    assert_eq!(size_of(&app, tab), (100, 30));
    app.forget(A);
    app.resize(AREA);
    assert_eq!(size_of(&app, tab), window_pane(&app));
    assert_eq!(a.id + 1, b.id);
}

#[test]
fn views_without_a_size_or_inactive_claim_nothing_but_follow_the_session() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 2)[1];
    let follower = open(&mut app, A, None, Some(tab));
    let idle = {
        let open =
            OpenView { cols: Some(50), rows: Some(10), active: Some(false), tab: Some(tab), ..OpenView::default() };
        let answer = ok(call(&mut app, B, None, Command::View(open), false));
        serde_json::from_value::<ViewInfo>(answer["view"].clone()).expect("a view")
    };
    app.resize(AREA);

    assert_eq!(size_of(&app, tab), window_pane(&app));
    assert!(error_of(&surface(&mut app, 80, follower.id)).contains("no size"));
    assert!(error_of(&surface(&mut app, 81, idle.id)).contains("inactive"));
    snapshots(&mut app, A);
    done(Response::Ok(ok(call(
        &mut app,
        71,
        None,
        Command::Rename(wire::Rename { item: Some(Item::Tab(tab)), name: "renamed".into() }),
        false,
    ))));
    let got = snapshots(&mut app, A);
    assert!(got.iter().any(|s| s["projects"][0]["workspaces"][0]["tabs"][1]["name"] == "renamed"), "{got:?}");
}

fn error_of(value: &Value) -> String {
    value["error"]["message"].as_str().unwrap_or_default().to_string()
}

#[test]
fn the_window_takes_back_the_tab_it_shows_when_it_is_used_and_a_lapsed_claim_returns() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 2);
    let view = open(&mut app, A, Some((70, 15)), Some(ids[0]));
    app.resize(AREA);
    assert_eq!(size_of(&app, ids[0]), (70, 15));

    app.window_used();
    assert_eq!(size_of(&app, ids[0]), window_pane(&app));
    assert!(!info(&mut app, view.id).controls);

    app.projects[0].workspaces[0].active = 1;
    app.resize(AREA);
    app.projects[0].workspaces[0].active = 0;
    app.resize(AREA);
    assert_eq!(size_of(&app, ids[0]), (70, 15));
    assert!(info(&mut app, view.id).controls);
}

#[test]
fn legacy_focus_moves_the_window_and_not_the_view() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 2);
    let view = open(&mut app, A, Some((70, 15)), Some(ids[0]));

    let answer =
        ok(call(&mut app, 71, None, Command::Focus(wire::Focus { item: Item::Tab(ids[1]), reveal: None }), false));

    assert_eq!(answer, serde_json::json!({}));
    assert_eq!(report(&mut app).shown.tab, Some(ids[1]));
    assert_eq!(info(&mut app, view.id).shown.tab, Some(ids[0]));
}

#[test]
fn a_tui_click_past_a_smaller_terminal_is_clamped_to_its_last_column() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    use crate::app::panes::{pane_cell, within_terminal};

    let (mut app, _rx, _dirs) = app_with(1);
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    open(&mut app, A, Some((10, 5)), Some(tab));
    app.resize(AREA);
    let pane = app.layout(AREA).pane;
    let click = |column, row| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };

    let rect = within_terminal(term(&app, 0), pane);
    let past = pane_cell(rect, click(pane.x + 60, pane.y + 12)).expect("a cell");
    let inside = pane_cell(rect, click(pane.x + 3, pane.y + 2)).expect("a cell");

    assert_eq!((past.x, past.y), (9, 4));
    assert_eq!((inside.x, inside.y), (3, 2));
    assert_eq!(size_of(&app, tab), (10, 5));
}

#[test]
fn a_view_surface_follows_the_view_and_frames_name_the_projection() {
    let (mut app, _rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 3);
    let view = open(&mut app, A, Some((60, 12)), Some(ids[1]));
    let first = ok(surface(&mut app, 80, view.id));
    let t0 = Instant::now();
    assert_eq!(
        (first["tab"].as_u64(), first["cols"].as_u64(), first["view"].as_u64()),
        (Some(ids[1]), Some(60), Some(view.id))
    );
    assert_eq!(first["projection"], 1);

    assert_eq!(frames_for(&mut app, 80, t0 + FRAME_EVERY).len(), 0);
    let moved = ok(focus(&mut app, view.id, A, ids[2]));
    let got = frames_for(&mut app, 80, t0 + FRAME_EVERY * 2);

    assert_eq!(moved["projection"], 2);
    assert_eq!((got.len(), got[0].tab, got[0].projection, got[0].view), (1, ids[2], Some(2), Some(view.id)));
    assert_eq!(got[0].content.panes[0].size.cols, 60);
}

#[test]
fn a_paused_surface_streams_nothing_and_does_not_keep_the_loop_awake() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 2)[1];
    let view = open(&mut app, A, Some((60, 12)), Some(tab));
    ok(surface(&mut app, 80, view.id));
    let t0 = Instant::now();
    set(&mut app, view.id, A, SetView { active: Some(false), ..SetView::default() });

    app.touch_surfaces();
    let got = frames_for(&mut app, 80, t0 + FRAME_EVERY);

    assert_eq!(got.len(), 0);
    assert_eq!(app.surface_wake(t0 + FRAME_EVERY), None);
    set(&mut app, view.id, A, SetView { active: Some(true), ..SetView::default() });
    assert_eq!(frames_for(&mut app, 80, t0 + FRAME_EVERY * 2).len(), 1);
}

#[test]
fn a_surface_ends_when_its_view_closes() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 2)[1];
    let view = open(&mut app, A, Some((60, 12)), Some(tab));
    ok(surface(&mut app, 80, view.id));

    app.forget(A);
    let ended = app.render_surfaces(&[80], Instant::now() + FRAME_EVERY);

    assert_eq!(ended, [(80, Surfaced::End)]);
}

#[test]
fn closing_the_views_tab_settles_it_on_a_neighbour_without_resizing_for_it() {
    let (mut app, rx, _dirs) = app_with(1);
    let ids = tabs(&mut app, 3);
    let one = open(&mut app, A, Some((60, 12)), Some(ids[1]));
    let other = open(&mut app, B, Some((50, 10)), Some(ids[2]));
    app.resize(AREA);

    done(Response::Ok(ok(call(
        &mut app,
        71,
        None,
        Command::Close(wire::Close { item: Item::Tab(ids[1]), remove_worktree: false, force: false, unlock: false }),
        false,
    ))));
    refreshing(&mut app, &rx, "the tab closes", |app| app.projects.tab_index(ids[1]).is_none());

    let now = info(&mut app, one.id);
    assert_eq!((now.shown.tab, now.projection), (Some(ids[0]), 2));
    assert_eq!(info(&mut app, other.id).shown.tab, Some(ids[2]));
    assert_eq!(size_of(&app, ids[0]), window_pane(&app));
    assert!(!now.controls);
}

#[rstest]
#[case::no_view(Command::ViewSet(SetView::default()), None, Code::Malformed)]
#[case::unknown_view(Command::Status(wire::Status {}), Some(9999), Code::NotFound)]
#[case::takes_no_view(Command::Stats(wire::Stats::default()), Some(1), Code::Malformed)]
#[case::view_in_a_view(Command::View(OpenView::default()), Some(1), Code::Malformed)]
#[case::half_a_size(Command::View(OpenView { cols: Some(10), ..OpenView::default() }), None, Code::Malformed)]
#[case::too_big(Command::View(OpenView { cols: Some(5000), rows: Some(10), ..OpenView::default() }), None, Code::Malformed)]
fn a_view_request_is_refused(#[case] command: Command, #[case] view: Option<u64>, #[case] expected: Code) {
    let (mut app, _rx, _dirs) = app_with(1);
    let held = open(&mut app, A, None, None);
    let view = view.map(|id| if id == 1 { held.id } else { id });

    let answer = call(&mut app, B, view, command, true);

    assert_eq!(code(&answer), serde_json::to_value(expected).expect("a code"), "{answer}");
}

#[test]
fn a_connection_holds_one_view_and_a_surface_takes_no_tab_or_size() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    let view = open(&mut app, A, Some((60, 12)), Some(tab));

    let second = call(&mut app, A, None, Command::View(OpenView::default()), true);
    let sized = call(
        &mut app,
        80,
        Some(view.id),
        Command::Surface(Surface { tab: Some(tab), cols: 80, rows: 24, patches: false, popup: false }),
        true,
    );

    assert_eq!(code(&second), "failed");
    assert_eq!(code(&sized), "malformed");
}

#[test]
fn a_view_is_one_more_id_and_its_lines_go_to_its_connection() {
    let (mut app, _rx, _dirs) = app_with(1);
    let view = open(&mut app, A, Some((60, 12)), None);
    let before = snapshots(&mut app, A).len() as u64;

    set(&mut app, view.id, A, SetView { cols: Some(70), rows: Some(14), ..SetView::default() });
    let got = snapshots(&mut app, A);

    let last = got.last().expect("a line after the change");
    assert_eq!(last["revision"].as_u64(), Some(1 + before + got.len() as u64));
    assert_eq!((last["view"]["cols"].as_u64(), last["view"]["projection"].as_u64()), (Some(70), Some(2)));
}

fn row_text(buffer: &Buffer, y: u16, from: u16, to: u16) -> String {
    (from..to).map(|x| buffer[(x, y)].symbol().to_string()).collect()
}

#[rstest]
#[case::smaller_than_the_window(20, 5)]
#[case::larger_than_the_window(150, 40)]
fn the_window_draws_a_tab_sized_by_a_view_within_its_own_pane(#[case] cols: u16, #[case] rows: u16) {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    let pane = app.projects[0].workspaces[0].tabs[0].panes[0].id;
    open(&mut app, A, Some((cols, rows)), Some(tab));
    app.resize(AREA);
    let text: String = (0..cols * 2).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    app.handle_event(AppEvent::Output(pane, 0, format!("\x1b[H{text}").into_bytes().into()), AREA).expect("output");

    let (buffer, _) = render_frame(&mut app, AREA, &Sight::default());

    let area = app.layout(AREA).pane;
    let shown = cols.min(area.width);
    let first = row_text(&buffer, area.y, area.x, area.x + shown);
    assert_eq!(first, text.chars().take(usize::from(shown)).collect::<String>());
    if cols < area.width {
        assert_eq!(buffer[(area.x + cols, area.y)].symbol(), " ");
    }
    assert_eq!(size_of(&app, tab), (cols, rows));
}

#[test]
fn a_rejected_claim_changes_nothing_and_the_window_leaving_hands_the_tab_back() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    let view = open(&mut app, A, None, Some(tab));

    let refused = set(&mut app, view.id, A, SetView { claim: true, active: Some(false), ..SetView::default() });
    let after = info(&mut app, view.id);
    assert_eq!(code(&refused), "failed");
    assert_eq!((after.active, after.projection), (true, 1));

    set(&mut app, view.id, A, SetView { cols: Some(30), rows: Some(8), ..SetView::default() });
    app.window_used();
    assert!(!info(&mut app, view.id).controls);
    app.window_gone();
    assert!(info(&mut app, view.id).controls);
    assert_eq!(size_of(&app, tab), (30, 8));
}

#[test]
fn a_view_change_reaches_the_view_at_once_but_a_watcher_keeps_its_pacing() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    let view = open(&mut app, A, Some((60, 12)), Some(tab));
    ok(call(&mut app, 77, None, Command::Watch(wire::Watch::default()), false));
    app.clients.events.snapshots.set_every(Duration::from_secs(60));
    app.trace();
    app.take_events();

    set(&mut app, view.id, A, SetView { cols: Some(70), rows: Some(14), ..SetView::default() });
    app.trace();

    let got = app.take_events();
    assert!(got.iter().any(|(client, _)| *client == A), "{got:?}");
    assert!(got.iter().all(|(client, _)| *client != 77), "{got:?}");
}

#[test]
fn a_view_set_hidden_says_so_and_changes_neither_projection_nor_control() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 1)[0];
    let opened = open(&mut app, A, Some((100, 30)), Some(tab));
    assert!(opened.visible);

    let hidden = ok(set(&mut app, opened.id, A, SetView { visible: Some(false), ..SetView::default() }));

    assert_eq!(hidden["visible"], false);
    assert_eq!((hidden["projection"].clone(), hidden["controls"].clone()), (opened.projection.into(), true.into()));
    assert!(!info(&mut app, opened.id).visible);
}

#[test]
fn go_through_the_socket_answers_a_projection_and_refuses_an_unknown_token() {
    let (mut app, _rx, _dirs) = app_with(1);
    let tab = tabs(&mut app, 1)[0];
    let opened = open(&mut app, A, Some((100, 30)), Some(tab));
    let go = || Command::Go(wire::Go { token: "cc-nope-1".into() });

    let refused = call(&mut app, B, Some(opened.id), go(), true);

    assert_eq!(code(&refused), "not-found");
    assert!(error_of(&refused).contains("there is no notice cc-nope-1"), "{refused}");
}
