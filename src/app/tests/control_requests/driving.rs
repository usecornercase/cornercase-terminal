use ratatui::style::Modifier;
use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::terminal::{Sent, Why};
use crate::project::Hierarchy;
use crate::project::split::Dir;

struct Refused {
    code: String,
    message: String,
}

fn call(app: &mut App, command: &str, args: &Value) -> std::result::Result<Value, Refused> {
    call_as(app, command, args, None)
}

fn call_as(app: &mut App, command: &str, args: &Value, boot: Option<&str>) -> std::result::Result<Value, Refused> {
    let text = json!({"contract": 1, "boot": boot, "command": command, "args": args}).to_string();
    app.request(CLIENT, &text, Some(AREA), Instant::now());
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    let (_, text) = answers.remove(0);
    let mut reply: Value = serde_json::from_str(&text).expect("a reply");
    match reply["error"].take() {
        Value::Null => Ok(reply["ok"].take()),
        error => Err(Refused {
            code: error["code"].as_str().unwrap_or_default().to_string(),
            message: error["message"].as_str().unwrap_or_default().to_string(),
        }),
    }
}

fn ok(app: &mut App, command: &str, args: &Value) -> Value {
    call(app, command, args).unwrap_or_else(|e| panic!("{command} refused: {} {}", e.code, e.message))
}

fn sent(app: &mut App, pane: u64, events: &Value) -> Sent {
    serde_json::from_value(ok(app, "input", &json!({"pane": pane, "events": events}))).expect("sent")
}

fn refused(app: &mut App, command: &str, args: &Value) -> Refused {
    match call(app, command, args) {
        Ok(value) => panic!("{command} was taken: {value}"),
        Err(refused) => refused,
    }
}

fn hidden_tab(app: &mut App) -> u64 {
    let active = app.active;
    let workspace = app.projects[active].active;
    let tab = app.push_tab(active, workspace, AREA, None).expect("a hidden tab");
    app.projects[active].workspaces[workspace].tabs[tab].pane().expect("a pane").id
}

fn line_in(pane: u64, text: &'static str) -> Value {
    let _ = pane;
    json!([{"text": text}, {"key": {"code": "enter"}}])
}

fn raw_cat(app: &mut App, rx: &Receiver<AppEvent>, pane: u64, setup: &'static str) {
    let line = format!("stty raw -echo; {setup}echo ready-\"\"now; cat -v");
    let line = Box::leak(line.into_boxed_str());
    sent(app, pane, &line_in(pane, line));
    pump_until(app, rx, "cat runs", |a| pane_screen(a, pane).contains("ready-now"));
}

#[test]
fn input_reaches_a_pane_of_a_hidden_tab_while_a_dialog_is_open() {
    let (mut app, rx) = app();
    let shown = first(&app);
    let hidden = hidden_tab(&mut app);
    app.open_settings();

    let answer = sent(&mut app, hidden, &line_in(hidden, "echo typed-$((20+22))"));

    pump_until(&mut app, &rx, "the hidden pane runs it", |a| pane_screen(a, hidden).contains("typed-42"));
    assert!(answer.bytes > 0);
    assert_eq!(answer.skipped, Vec::new());
    assert!(matches!(app.overlay, Some(Overlay::Settings(_))), "the dialog stays");
    assert_eq!(first(&app), shown, "the window still shows its tab");
    assert!(!pane_screen(&app, shown).contains("typed-42"));
}

#[rstest]
#[case::ctrl("ctrl", "b", "^B")]
#[case::arrow("none", "up", "^[[A")]
fn keys_follow_the_programs_modes(#[case] modifier: &str, #[case] code: &str, #[case] shows: &str) {
    let (mut app, rx) = app();
    let id = first(&app);
    raw_cat(&mut app, &rx, id, "");
    let mods: Vec<&str> = if modifier == "none" { vec![] } else { vec![modifier] };

    sent(&mut app, id, &json!([{"key": {"code": code, "mods": mods}}]));

    pump_until(&mut app, &rx, "cat shows the bytes", |a| pane_screen(a, id).contains(shows));
}

#[test]
fn application_cursor_keys_change_the_arrow() {
    let (mut app, rx) = app();
    let id = first(&app);
    raw_cat(&mut app, &rx, id, "printf '\\033[?1h'; ");

    sent(&mut app, id, &json!([{"key": {"code": "up"}}]));

    pump_until(&mut app, &rx, "the application arrow", |a| pane_screen(a, id).contains("^[OA"));
}

#[test]
fn releases_are_encoded_only_when_the_program_asks_for_event_types() {
    let (mut app, rx) = app();
    let (plain, asking) = (first(&app), hidden_tab(&mut app));
    raw_cat(&mut app, &rx, plain, "");
    raw_cat(&mut app, &rx, asking, "printf '\\033[>11u'; ");
    let release = json!([{"key": {"code": "a", "kind": "release"}}]);

    let taken = sent(&mut app, plain, &release);
    let reported = sent(&mut app, asking, &release);

    assert_eq!((taken.bytes, taken.skipped.len(), taken.skipped[0].why), (0, 1, Why::Release));
    assert_eq!(reported.skipped, Vec::new());
    pump_until(&mut app, &rx, "the release shows", |a| pane_screen(a, asking).contains(":3u"));
    assert!(!pane_screen(&app, plain).contains("97"), "{}", pane_screen(&app, plain));
}

#[test]
fn committed_text_types_like_the_window() {
    let (mut app, rx) = app();
    let id = first(&app);
    raw_cat(&mut app, &rx, id, "");

    sent(&mut app, id, &json!([{"text": "héllo"}]));

    pump_until(&mut app, &rx, "the text shows", |a| pane_screen(a, id).contains("M-CM-)llo"));
}

#[test]
fn a_control_character_in_text_is_refused_and_nothing_is_sent() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let refusal = refused(&mut app, "input", &json!({"pane": id, "events": [{"text": "a\nb"}]}));

    assert_eq!(refusal.code, "malformed", "{}", refusal.message);
    assert!(pane(&app, id).input_at.is_none());
}

#[rstest]
#[case::plain("", "two", "two")]
#[case::bracketed("printf '\\033[?2004h'; ", "two", "^[[200~two^[[201~")]
fn a_paste_is_bracketed_when_the_program_asks(#[case] setup: &'static str, #[case] text: &str, #[case] shows: &str) {
    let (mut app, rx) = app();
    let id = first(&app);
    raw_cat(&mut app, &rx, id, setup);

    sent(&mut app, id, &json!([{"paste": text}]));

    pump_until(&mut app, &rx, "the paste shows", |a| pane_screen(a, id).contains(shows));
}

#[test]
fn mouse_events_are_cells_of_the_pane_in_its_encoding() {
    let (mut app, rx) = app();
    let first_pane = first(&app);
    let second = app.split_pane(first_pane, Dir::Right, AREA, false).expect("split").expect("a pane");
    raw_cat(&mut app, &rx, second, "printf '\\033[?1002h\\033[?1006h'; ");

    let down = json!([{"mouse": {"kind": "down", "x": 0, "y": 0}}]);
    let drag = json!([{"mouse": {"kind": "drag", "button": "left", "x": 2, "y": 1, "mods": ["shift"]}}]);
    sent(&mut app, second, &down);
    sent(&mut app, second, &drag);

    pump_until(&mut app, &rx, "the cells show", |a| {
        let screen = pane_screen(a, second);
        screen.contains("^[[<0;1;1M") && screen.contains("^[[<36;3;2M")
    });
}

#[test]
fn mouse_events_the_program_does_not_read_are_skipped_and_said() {
    let (mut app, rx) = app();
    let id = first(&app);
    raw_cat(&mut app, &rx, id, "");
    let before = pane(&app, id).input_at;

    let only = sent(&mut app, id, &json!([{"mouse": {"kind": "down", "x": 1, "y": 1}}]));
    let mixed = sent(&mut app, id, &json!([{"mouse": {"kind": "down", "x": 1, "y": 1}}, {"text": "k"}]));

    assert_eq!((only.bytes, only.skipped.len(), only.skipped[0].why), (0, 1, Why::Mouse));
    assert_eq!((mixed.bytes, mixed.skipped.len(), mixed.skipped[0].event), (1, 1, 0));
    assert!(before.is_some());
}

#[rstest]
#[case::unknown_key(json!({"pane": 0, "events": [{"key": {"code": "nope"}}]}), "malformed")]
#[case::no_pane(json!({"events": [{"text": "x"}]}), "malformed")]
#[case::unknown_pane(json!({"pane": 9999, "events": [{"text": "x"}]}), "not-found")]
#[case::outside(json!({"pane": 0, "events": [{"text": "x"}, {"mouse": {"x": 500, "y": 0}}]}), "conflict")]
#[case::other_size(json!({"pane": 0, "size": {"cols": 3, "rows": 3}, "events": [{"text": "x"}]}), "conflict")]
#[case::no_events(json!({"pane": 0, "events": []}), "malformed")]
fn rejected_input_sends_nothing(#[case] mut args: Value, #[case] code: &str) {
    let (mut app, _rx) = app();
    let id = first(&app);
    if args["pane"] == 0 {
        args["pane"] = json!(id);
    }

    let refusal = refused(&mut app, "input", &args);

    assert_eq!(refusal.code, code, "{}", refusal.message);
    assert!(pane(&app, id).input_at.is_none(), "nothing was written");
}

#[test]
fn stale_boot_input_is_refused_before_anything_is_typed() {
    let (mut app, _rx) = app();
    let id = first(&app);

    let refusal = match call_as(&mut app, "input", &json!({"pane": id, "events": [{"text": "x"}]}), Some("1-other")) {
        Ok(value) => panic!("typed: {value}"),
        Err(refusal) => refusal,
    };

    assert_eq!(refusal.code, "stale-boot");
    assert!(pane(&app, id).input_at.is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn a_full_pane_answers_busy_and_takes_nothing() {
    let (mut app, rx) = app();
    let id = first(&app);
    type_line(&mut app, "stty -echo; sleep 30");
    pump_until(&mut app, &rx, "sleep runs", |a| pane(a, id).program(&a.config).as_deref() == Some("sleep"));
    let full = vec![b'x'; crate::term::MAX_QUEUED + 1024 * 1024];
    assert!(app.projects[0].workspaces[0].tabs[0].panes[0].write(&full));

    let refusal = refused(&mut app, "input", &json!({"pane": id, "events": [{"text": "x"}]}));
    let old = error(now(&mut app, None, press_keys(id, &["enter"])));

    assert_eq!(refusal.code, "busy", "{}", refusal.message);
    assert!(refusal.message.contains("nothing was sent"));
    assert!(old.contains("is not reading its input"), "send and keys keep their text: {old}");
}

#[test]
fn scroll_moves_the_shared_view_and_says_where_it_is() {
    let (mut app, _rx) = app();
    let id = first(&app);
    numbered(app.projects.pane_by_mut(id).expect("pane"), 100);
    let at = |app: &mut App, args: Value| -> (u64, u64) {
        let value = ok(app, "scroll", &args);
        (value["scroll"]["offset"].as_u64().expect("offset"), value["scroll"]["total"].as_u64().expect("total"))
    };

    let (bottom, total) = at(&mut app, json!({"pane": id, "to": "bottom"}));
    let (up, _) = at(&mut app, json!({"pane": id, "lines": -10}));
    let (top, _) = at(&mut app, json!({"pane": id, "to": "top"}));
    let (down, _) = at(&mut app, json!({"pane": id, "lines": 3}));

    assert_eq!((up, top, down), (bottom - 10, 0, 3));
    assert!(total >= 100);
    assert_eq!(pane(&app, id).emulator.scrollbar().offset, 3);
}

#[test]
fn scroll_is_refused_where_the_program_scrolls_itself() {
    let (mut app, _rx) = app();
    let id = first(&app);
    let term = app.projects.pane_by_mut(id).expect("pane");
    term.feed(b"\x1b[?1049h");
    let alternate = refused(&mut app, "scroll", &json!({"pane": id, "lines": -1}));
    app.projects.pane_by_mut(id).expect("pane").feed(b"\x1b[?1049l\x1b[?1000h");
    let mouse = refused(&mut app, "scroll", &json!({"pane": id, "lines": -1}));
    app.projects.pane_by_mut(id).expect("pane").feed(b"\x1b[?1000l");
    let zero = refused(&mut app, "scroll", &json!({"pane": id, "lines": 0}));
    let both = refused(&mut app, "scroll", &json!({"pane": id, "lines": 1, "to": "top"}));

    assert_eq!(
        [&alternate.code, &mouse.code, &zero.code, &both.code],
        ["conflict", "conflict", "malformed", "malformed"]
    );
}

#[test]
fn selection_selects_reads_copies_and_clears() {
    let (mut app, _rx) = app();
    let id = first(&app);
    app.projects.pane_by_mut(id).expect("pane").feed(b"\x1b[2J\x1b[Hhello world\r\nsecond");
    let select = json!({"pane": id, "from": {"x": 0, "y": 0}, "to": {"x": 4, "y": 0}});

    let chosen = ok(&mut app, "selection", &select);
    let read = ok(&mut app, "selection", &json!({"pane": id}));
    let marked = app.projects.pane_by_mut(id).expect("pane").emulator.snapshot().expect("snapshot");
    let block = ok(
        &mut app,
        "selection",
        &json!({"pane": id, "rectangle": true, "from": {"x": 0, "y": 0}, "to": {"x": 1, "y": 1}}),
    );
    ok(&mut app, "selection", &select);
    app.take_host_writes();
    ok(&mut app, "selection", &json!({"pane": id, "copy": true}));
    let copied = app.take_host_writes();
    ok(&mut app, "selection", &json!({"pane": id, "clear": true}));
    let cleared = ok(&mut app, "selection", &json!({"pane": id}));
    let nothing = refused(&mut app, "selection", &json!({"pane": id, "copy": true}));
    let outside =
        refused(&mut app, "selection", &json!({"pane": id, "from": {"x": 0, "y": 0}, "to": {"x": 999, "y": 0}}));
    let half = refused(&mut app, "selection", &json!({"pane": id, "from": {"x": 0, "y": 0}}));

    assert_eq!((chosen["text"].as_str(), read["text"].as_str()), (Some("hello"), Some("hello")));
    assert!(marked.rows[0][0].style.add_modifier.contains(Modifier::REVERSED));
    assert_eq!(block["text"].as_str(), Some("he\nse"));
    assert_eq!(copied.len(), 1);
    assert!(cleared["text"].is_null(), "{cleared}");
    assert_eq!(
        (nothing.code.as_str(), outside.code.as_str(), half.code.as_str()),
        ("conflict", "conflict", "malformed")
    );
}

#[test]
fn a_ratio_moves_a_divider_and_the_terminals_follow() {
    let (mut app, _rx) = app();
    let left = first(&app);
    let right = app.split_pane(left, Dir::Right, AREA, false).expect("split").expect("a pane");
    let tab = app.projects[0].workspaces[0].tabs[0].id;
    let cols = |app: &App, id| pane(app, id).emulator.size().expect("size").1;
    let before = cols(&app, left);

    let moved = ok(&mut app, "ratio", &json!({"tab": tab, "path": [], "ratio": 0.25}));

    assert_eq!(moved["layout"]["split"]["ratio"].as_f64(), Some(0.25));
    assert!(cols(&app, left) < before && cols(&app, left) < cols(&app, right));
    let unknown = refused(&mut app, "ratio", &json!({"tab": tab, "path": [true], "ratio": 0.5}));
    let low = refused(&mut app, "ratio", &json!({"tab": tab, "ratio": 0.0}));
    let high = refused(&mut app, "ratio", &json!({"tab": tab, "ratio": 1.5}));
    let none = refused(&mut app, "ratio", &json!({"tab": 9999, "ratio": 0.5}));
    assert_eq!(
        [&unknown.code, &low.code, &high.code, &none.code],
        ["not-found", "malformed", "malformed", "not-found"]
    );
}

#[test]
fn an_empty_paste_sends_nothing_and_leaves_the_view_where_it_is() {
    let (mut app, _rx) = app();
    let id = first(&app);
    numbered(app.projects.pane_by_mut(id).expect("pane"), 100);
    app.projects.pane_by_mut(id).expect("pane").emulator.scroll(-5);
    let before = pane(&app, id).emulator.scrollbar().offset;

    let answer = sent(&mut app, id, &json!([{"paste": ""}]));

    assert_eq!(answer.bytes, 0);
    assert_eq!(pane(&app, id).emulator.scrollbar().offset, before);
}
