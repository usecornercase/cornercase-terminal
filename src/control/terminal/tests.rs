use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};

fn request(args: &Value) -> Request {
    serde_json::from_value(json!({"command": "input", "args": args})).expect("a request")
}

#[test]
fn every_event_shape_reads_and_is_written_back() {
    let args = json!({"pane": 12, "popup": false, "size": {"cols": 80, "rows": 24}, "events": [
        {"key": {"code": "c", "mods": ["ctrl"], "kind": "release"}},
        {"text": "héllo"},
        {"paste": "two\nlines"},
        {"mouse": {"kind": "scroll-up", "button": "left", "x": 3, "y": 1, "mods": ["shift", "super"]}},
    ]});

    let parsed = request(&args);

    assert_eq!(serde_json::to_value(&parsed).expect("json")["args"], args);
}

#[test]
fn a_redraw_request_names_its_surface_and_logs_it() {
    let parsed: Request =
        serde_json::from_value(json!({"command": "redraw", "args": {"surface": 4}})).expect("a request");

    assert_eq!(parsed.command.fields(), [("surface", "4".to_string())]);
    assert_eq!(serde_json::to_value(&parsed).expect("json")["args"], json!({"surface": 4}));
    assert_eq!(parsed.command.name(), "redraw");
}

#[test]
fn a_key_or_mouse_event_takes_its_defaults() {
    let parsed = request(&json!({"pane": 1, "events": [{"key": {"code": "enter"}}, {"mouse": {"x": 1}}]}));

    let Command::Input(input) = parsed.command else { panic!("not input") };
    assert_eq!(
        input.events,
        [
            InputEvent::Key(Key { code: "enter".into(), mods: vec![], kind: KeyKind::Press }),
            InputEvent::Mouse(Mouse { x: 1, ..Mouse::default() }),
        ]
    );
}

#[test]
fn an_unknown_event_does_not_read() {
    let read = serde_json::from_value::<Request>(json!({"command": "input", "args": {"events": [{"focus": true}]}}));

    assert!(read.is_err());
}

#[test]
fn a_stray_field_under_an_event_is_found_by_its_path() {
    let raw = json!({"command": "input", "args": {"pane": 1, "events": [{"key": {"code": "c", "bogus": 1}}]}});
    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");

    assert_eq!(unknown_fields(&raw, &parsed).as_deref(), Some("args.events.0.key.bogus"));
}

#[rstest]
#[case::down(MouseKind::Down, MouseEventKind::Down(MouseButton::Right))]
#[case::up(MouseKind::Up, MouseEventKind::Up(MouseButton::Right))]
#[case::drag(MouseKind::Drag, MouseEventKind::Drag(MouseButton::Right))]
#[case::moved(MouseKind::Moved, MouseEventKind::Moved)]
#[case::up_wheel(MouseKind::ScrollUp, MouseEventKind::ScrollUp)]
#[case::down_wheel(MouseKind::ScrollDown, MouseEventKind::ScrollDown)]
#[case::left_wheel(MouseKind::ScrollLeft, MouseEventKind::ScrollLeft)]
#[case::right_wheel(MouseKind::ScrollRight, MouseEventKind::ScrollRight)]
fn a_mouse_event_maps_to_crossterms(#[case] kind: MouseKind, #[case] expected: MouseEventKind) {
    let mouse = Mouse { kind, button: Button::Right, x: 4, y: 2, mods: vec![Modifier::Alt] };

    let event = mouse.event();

    assert_eq!((event.kind, event.column, event.row, event.modifiers), (expected, 4, 2, KeyModifiers::ALT));
}

#[test]
fn a_key_names_its_modifiers_and_kind() {
    let key = Key { code: "a".into(), mods: vec![Modifier::Ctrl, Modifier::Super], kind: KeyKind::Repeat };

    let event = key.event().expect("a key");

    assert_eq!(event.modifiers, KeyModifiers::CONTROL | KeyModifiers::SUPER);
    assert_eq!(event.kind, KeyEventKind::Repeat);
    assert_eq!(Key { code: "nope".into(), ..Key::default() }.event(), Err("unknown key `nope`".to_string()));
}

#[test]
fn the_fields_to_log_are_counts_and_sizes() {
    let input = Input {
        pane: Some(3),
        popup: false,
        size: None,
        events: vec![
            InputEvent::Text("secret".into()),
            InputEvent::Paste("secret!".into()),
            InputEvent::Key(Key::default()),
            InputEvent::Mouse(Mouse::default()),
        ],
    };

    let fields = Command::Input(input).fields();

    let shown: Vec<String> = fields.iter().map(|(key, value)| format!("{key}={value}")).collect();
    assert_eq!(shown, ["pane=3", "events=4", "keys=1", "mice=1", "text_bytes=6", "paste_bytes=7"]);
}

#[test]
fn a_link_request_reads_and_is_written_back_and_logs_its_cell() {
    let args = json!({"pane": 12, "size": {"cols": 80, "rows": 24}, "cell": {"x": 4, "y": 0}});
    let parsed: Request = serde_json::from_value(json!({"command": "link", "args": args})).expect("a request");

    assert_eq!(serde_json::to_value(&parsed).expect("json")["args"], args);
    assert_eq!(parsed.command.fields(), [("pane", "12".to_string()), ("x", "4".to_string()), ("y", "0".to_string())]);
    let raw = json!({"command": "link", "args": {"pane": 1, "cell": {"x": 1, "y": 2, "z": 3}}});
    let stray: Request = serde_json::from_value(raw.clone()).expect("a request");
    assert_eq!(unknown_fields(&raw, &stray).as_deref(), Some("args.cell.z"));
}

#[rstest]
#[case::a_file(None, "/src/a.rs")]
#[case::a_line(Some((7, 7)), "/src/a.rs:7")]
#[case::a_range(Some((7, 9)), "/src/a.rs:7-9")]
fn a_location_is_the_path_with_its_lines(#[case] lines: Option<(u32, u32)>, #[case] expected: &str) {
    let link = FileLink {
        path: "/src/a.rs".into(),
        lines: lines.map(|(first, last)| Lines { first, last }),
        ..FileLink::default()
    };

    assert_eq!(link.location(), expected);
}
