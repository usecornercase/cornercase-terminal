use serde_json::Value;

use super::*;
use crate::control::surface::mirror::Mirror;
use crate::control::terminal::Redraw;

const PEER: u64 = CLIENT + 1;

fn patching(tab: u64) -> Command {
    Command::Surface(Surface { tab: Some(tab), cols: 80, rows: 24, patches: true })
}

fn subscribe_patching(app: &mut App, client: u64, at: Instant, tab: u64) -> String {
    subscribe_raw(app, client, at, &patching(tab))
}

fn subscribe_raw(app: &mut App, client: u64, at: Instant, command: &Command) -> String {
    app.request(client, &text_of(command, None, false), Some(AREA), at);
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    answers.remove(0).1
}

fn texts_for(rendered: &[(u64, Surfaced)], client: u64) -> Vec<String> {
    rendered
        .iter()
        .filter(|(who, _)| *who == client)
        .filter_map(|(_, surfaced)| match surfaced {
            Surfaced::Frame(text) => Some(text.clone()),
            Surfaced::End => None,
        })
        .collect()
}

fn lines_of(text: &str) -> Value {
    let value: Value = serde_json::from_str(text).expect("json");
    value["ok"].clone()
}

fn next(app: &mut App, client: u64, at: Instant) -> Vec<Value> {
    texts_for(&app.render_surfaces(&[CLIENT, PEER], at), client).iter().map(|text| lines_of(text)).collect()
}

fn redraw(app: &mut App, surface: Option<u64>, at: Instant) -> Response {
    subscribe_as(app, PEER + 1, at, &Command::Redraw(Redraw { surface }))
}

#[test]
fn the_first_frame_is_complete_and_the_next_one_a_patch_of_it() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    let first = lines_of(&subscribe_patching(&mut app, CLIENT, t0, tab));
    assert!(first.get("base").is_none() && first["revision"] == 1);

    output(&mut app, pane, "hi");
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY);

    assert_eq!(got.len(), 1);
    assert_eq!((got[0]["base"].as_u64(), got[0]["revision"].as_u64()), (Some(1), Some(2)));
    let changed = &got[0]["panes"][0];
    assert_eq!(changed["lines"].as_object().expect("rows").keys().collect::<Vec<_>>(), ["0"]);
    assert!(changed["cursor"].is_object());
    assert!(got[0].get("layout").is_none());
}

#[test]
fn a_surface_that_did_not_ask_never_gets_a_patch() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe(&mut app, t0, tab);

    output(&mut app, pane, "hi");
    let got = ready(&mut app, t0 + FRAME_EVERY);

    assert_eq!(got.len(), 1);
    assert_eq!(line(&got[0], 0, 0), "hi");
}

#[test]
fn printing_past_the_bottom_copies_rows_instead_of_sending_them() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe_patching(&mut app, CLIENT, t0, tab);
    let rows = usize::from(
        app.projects[0].workspaces[0].tabs[0].pane_by_mut(pane).expect("a pane").emulator.size().expect("a size").0,
    );
    let fill: String = (0..rows + 3).fold(String::new(), |all, n| all + &format!("line {n}\r\n"));
    output(&mut app, pane, &fill);
    next(&mut app, CLIENT, t0 + FRAME_EVERY);

    output(&mut app, pane, "one more\r\n");
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY * 2);

    let changed = &got[0]["panes"][0];
    assert!(changed["copy"].is_object(), "{changed}");
    assert!(changed["lines"].as_object().expect("rows").len() <= 3, "{changed}");
}

#[test]
fn a_split_sends_a_complete_frame() {
    let (mut app, rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe_patching(&mut app, CLIENT, t0, tab);

    done(now(&mut app, None, Command::Split(wire::Split { pane: Some(pane), ..wire::Split::default() })));
    drop(rx);
    app.touch_surfaces();
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY);

    assert!(got[0].get("base").is_none());
    assert_eq!((got[0]["revision"].as_u64(), got[0]["panes"].as_array().map(Vec::len)), (Some(2), Some(2)));
}

#[test]
fn a_resize_sends_a_complete_frame() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (tab, t0) = (tab_of(&app, 0), Instant::now());
    let surface = lines_of(&subscribe_patching(&mut app, CLIENT, t0, tab))["surface"].as_u64();

    subscribe_as(
        &mut app,
        PEER,
        t0,
        &Command::Resize(crate::control::terminal::Resize { surface, cols: 100, rows: 30 }),
    );
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY);

    assert!(got[0].get("base").is_none());
    assert_eq!((got[0]["revision"].as_u64(), got[0]["cols"].as_u64()), (Some(2), Some(100)));
}

#[test]
fn a_reader_without_room_gets_a_complete_frame_once_it_has_room() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe_patching(&mut app, CLIENT, t0, tab);

    output(&mut app, pane, "one");
    let none = texts_for(&app.render_surfaces(&[], t0 + FRAME_EVERY), CLIENT);
    output(&mut app, pane, "two");
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY * 2);

    assert_eq!(none, Vec::<String>::new());
    assert_eq!(got.len(), 1);
    assert!(got[0].get("base").is_none(), "{}", got[0]);
    assert_eq!(got[0]["revision"], 2);
    let patched = next(&mut app, CLIENT, t0 + FRAME_EVERY * 3);
    assert_eq!(patched, Vec::<Value>::new());
}

#[test]
fn a_reader_that_missed_nothing_gets_no_frame_later() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe_patching(&mut app, CLIENT, t0, tab);

    let skipped = texts_for(&app.render_surfaces(&[], t0 + REFRESH_EVERY), CLIENT);
    let later = next(&mut app, CLIENT, t0 + REFRESH_EVERY * 2);
    output(&mut app, pane, "x");
    let changed = next(&mut app, CLIENT, t0 + REFRESH_EVERY * 3);

    assert!(skipped.is_empty() && later.is_empty());
    assert_eq!((changed[0]["base"].as_u64(), changed[0]["revision"].as_u64()), (Some(1), Some(2)));
}

#[test]
fn redraw_sends_a_complete_frame_at_once_even_when_nothing_changed() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (tab, t0) = (tab_of(&app, 0), Instant::now());
    let surface = lines_of(&subscribe_patching(&mut app, CLIENT, t0, tab))["surface"].as_u64();

    let Response::Ok(answer) = redraw(&mut app, surface, t0 + FRAME_EVERY) else { panic!("refused") };
    let got = next(&mut app, CLIENT, t0 + FRAME_EVERY);

    assert_eq!((answer["surface"].as_u64(), answer["tab"].as_u64()), (surface, Some(tab)));
    assert_eq!(got.len(), 1);
    assert!(got[0].get("base").is_none());
    assert_eq!(got[0]["revision"], 2);
}

#[rstest]
#[case::no_surface(None, Code::Malformed)]
#[case::unknown(Some(99), Code::NotFound)]
fn redraw_needs_a_live_surface(#[case] surface: Option<u64>, #[case] code: Code) {
    let (mut app, _rx, _dirs) = app_with(1);
    let command = Command::Redraw(Redraw { surface });

    app.request(CLIENT, &text_of(&command, None, true), Some(AREA), Instant::now());

    let answers = app.take_answers();
    let value: Value = serde_json::from_str(&answers[0].1).expect("json");
    assert_eq!(value["error"]["code"], serde_json::to_value(code).expect("a code"), "{value}");
}

#[test]
fn patches_rebuild_what_a_complete_stream_shows() {
    let (mut app, rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    let mut patched = Mirror::default();
    patched.apply(&subscribe_patching(&mut app, CLIENT, t0, tab)).expect("the first frame");
    let mut whole = Mirror::default();
    whole.apply(&subscribe_raw(&mut app, PEER, t0, &command(tab, 80, 24))).expect("the first frame");
    let rows: String = (0..30).fold(String::new(), |all, n| all + &format!("row {n}\r\n"));
    let steps = [
        "\x1b[1;32mgreen\x1b[0m plain".to_string(),
        rows,
        "\x1b]8;;https://example.com\x1b\\site\x1b]8;;\x1b\\".to_string(),
        "\x1b[3;5H\x1b[6 q".to_string(),
        "\x1b[2J\x1b[H".to_string(),
        "\x1b[4:3mundercurl\x1b[0m\r\nnext".to_string(),
    ];

    for (at, step) in steps.iter().enumerate() {
        output(&mut app, pane, step);
        let when = t0 + FRAME_EVERY * u32::try_from(at + 1).expect("a step");
        let rendered = app.render_surfaces(&[CLIENT, PEER], when);
        feed(&mut patched, &mut whole, &rendered);
        assert_eq!(patched.shown, whole.shown, "after step {at}");
    }
    done(now(&mut app, None, Command::Split(wire::Split { pane: Some(pane), ..wire::Split::default() })));
    drop(rx);
    app.touch_surfaces();
    let when = t0 + FRAME_EVERY * 20;
    feed(&mut patched, &mut whole, &app.render_surfaces(&[CLIENT, PEER], when));
    assert_eq!(patched.shown, whole.shown);
}

fn feed(patched: &mut Mirror, whole: &mut Mirror, rendered: &[(u64, Surfaced)]) {
    for text in texts_for(rendered, CLIENT) {
        patched.apply(&text).expect("a patch applies");
    }
    for text in texts_for(rendered, PEER) {
        whole.apply(&text).expect("a frame applies");
    }
}

#[test]
fn reconnecting_with_patches_starts_from_a_complete_frame() {
    let (mut app, _rx, _dirs) = app_with(1);
    let (pane, tab, t0) = (term(&app, 0).id, tab_of(&app, 0), Instant::now());
    subscribe_patching(&mut app, CLIENT, t0, tab);
    output(&mut app, pane, "kept");
    next(&mut app, CLIENT, t0 + FRAME_EVERY);

    app.forget(CLIENT);
    let again = lines_of(&subscribe_patching(&mut app, CLIENT, t0 + FRAME_EVERY * 2, tab));

    assert!(again.get("base").is_none());
    assert_eq!(again["revision"], 1);
}
