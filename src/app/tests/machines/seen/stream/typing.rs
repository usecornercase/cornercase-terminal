use super::*;
use crate::app::toast::NOT_READING;
use crate::control::terminal::{InputEvent, Key};

const UNREACHABLE: &str = "vps unreachable: nothing was sent";

fn on(streaming: &mut Streaming) -> &mut App {
    &mut streaming.seeing.rig.app
}

fn key(code: &str) -> InputEvent {
    InputEvent::Key(Key { code: code.into(), ..Key::default() })
}

fn events(asked: &Value) -> Vec<InputEvent> {
    serde_json::from_value(asked["args"]["events"].clone()).expect("input events")
}

fn typed(streaming: &mut Streaming) -> Value {
    request(&mut reach(&streaming.seeing.ends))
}

#[test]
fn a_key_goes_to_the_active_pane_there_at_any_size() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");

    type_text(on(&mut streaming), "a");

    let asked = typed(&mut streaming);
    assert_eq!((&asked["command"], &asked["args"]["pane"]), (&json!("input"), &json!(1)));
    assert_eq!((&asked["args"]["size"], events(&asked)), (&Value::Null, vec![key("a")]));
}

#[test]
fn a_key_typed_after_a_resize_before_the_next_frame_still_lands() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    on(&mut streaming).resize(WIDE);

    type_text(on(&mut streaming), "a");

    let mut orders = reach(&streaming.seeing.ends);
    let resized = request(&mut orders);
    say(&mut orders, &json!({"ok": {"id": VIEW, "projection": 2}}));
    let asked = request(&mut orders);
    assert_eq!((&resized["command"], &asked["command"]), (&json!("view-set"), &json!("input")));
    assert_eq!((&asked["args"]["size"], events(&asked)), (&Value::Null, vec![key("a")]));
}

#[test]
fn a_paste_goes_as_a_paste() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");

    on(&mut streaming).handle_paste("two\nlines", AREA);

    assert_eq!(events(&typed(&mut streaming)), [InputEvent::Paste("two\nlines".into())]);
}

#[test]
fn the_prefix_key_and_its_menu_send_nothing() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let app = on(&mut streaming);
    app.config.prefix_key = "ctrl+]".into();

    send_key(app, KeyCode::Char(']'), KeyModifiers::CONTROL);
    assert!(matches!(app.overlay, Some(Overlay::Keys(_))));
    send_key(app, KeyCode::Esc, KeyModifiers::NONE);
    type_text(app, "b");

    assert_eq!(events(&typed(&mut streaming)), [key("b")]);
}

#[test]
fn keys_into_a_dialog_send_nothing() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    let app = on(&mut streaming);
    app.overlay = Some(Overlay::Usage);

    type_text(app, "z");
    app.handle_paste("pasted", AREA);
    app.overlay = None;
    type_text(app, "c");

    assert_eq!(events(&typed(&mut streaming)), [key("c")]);
}

#[test]
fn before_the_first_frame_nothing_is_sent_or_kept_and_a_toast_says_so() {
    let mut streaming = streaming();

    type_text(on(&mut streaming), "a");
    on(&mut streaming).handle_paste("early", AREA);
    assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
    live(&mut streaming, "hello");
    type_text(on(&mut streaming), "b");

    assert_eq!(events(&typed(&mut streaming)), [key("b")]);
}

#[test]
fn with_the_link_down_nothing_is_sent_and_a_toast_says_so() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    fall(&mut streaming.seeing);

    type_text(on(&mut streaming), "a");

    assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
}

#[test]
fn a_program_there_not_reading_says_so_in_a_toast() {
    let mut streaming = streaming();
    live(&mut streaming, "hello");
    type_text(on(&mut streaming), "a");
    let mut orders = reach(&streaming.seeing.ends);
    request(&mut orders);

    say(&mut orders, &json!({"error": {"code": "busy", "message": "the program is not reading its input"}}));

    streaming.seeing.rig.until("the refusal arrives", |app| toast(app) == Some(NOT_READING));
}
