use std::time::Instant;

use super::*;
use crate::app::machines::MachineEvent;
use crate::app::toast::NOT_READING;
use crate::control::contract::{Code, Refusal};
use crate::control::terminal::{InputEvent, Key};
use crate::machines::stream::orders::{Answer, Order};
use crate::machines::stream::{ANSWERS_WITHIN, StreamEvent, StreamReport};

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

mod after_moving {
    use super::*;

    fn tab_with_pane(tab: u64, pane: u64, projection: u64) -> Value {
        let mut frame = There { tab, ..THERE }.complete(2, projection, "other");
        frame["ok"]["panes"][0]["pane"] = json!(pane);
        frame
    }

    fn kept(mut streaming: Streaming, text: &str) -> Streaming {
        assert!(on(&mut streaming).select_remote("vps", RemoteItem::Tab(11)));
        type_text(on(&mut streaming), text);
        streaming
    }

    fn kept_live(text: &str) -> Streaming {
        let mut streaming = streaming();
        live(&mut streaming, "hello");
        kept(streaming, text)
    }

    fn held(streaming: &mut Streaming) -> Vec<InputEvent> {
        on(streaming).watched().map(|watched| watched.typed.clone()).unwrap_or_default()
    }

    fn split(streaming: &mut Streaming) {
        say(&mut streaming.surface, &THERE.split(1, 1, "none"));
        streaming.seeing.rig.until("the split arrives", |app| {
            app.watched().and_then(|watched| watched.screen.as_ref()).is_some_and(|screen| screen.panes.len() == 2)
        });
    }

    fn lost(app: &mut App, order: Order) {
        let stream = app.watched().and_then(|watched| watched.live()).map(|(stream, _)| stream.id());
        let refusal = Refusal { code: Code::Unavailable, message: "the view is gone".into() };
        let event = StreamEvent::Answer(Answer::Refused { order, refusal });
        let report = StreamReport { stream: stream.expect("a live stream"), event };
        app.handle_event(AppEvent::Machine(MachineEvent::Stream(report)), AREA).expect("handle event");
    }

    #[test]
    fn a_key_typed_right_after_a_click_on_another_pane_goes_to_that_pane() {
        let mut streaming = streaming();
        split(&mut streaming);
        let pane = on(&mut streaming).layout(AREA).pane;
        click(on(&mut streaming), Position::new(pane.x + 13, pane.y + 1));

        type_text(on(&mut streaming), "a");

        let mut orders = reach(&streaming.seeing.ends);
        assert_eq!(request(&mut orders)["command"], "focus");
        say(&mut orders, &json!({"ok": {"projection": 2}}));
        let asked = request(&mut orders);
        assert_eq!(
            (&asked["command"], &asked["args"]["pane"], events(&asked)),
            (&json!("input"), &json!(2), vec![key("a")])
        );
    }

    #[test]
    fn keys_typed_while_another_tab_opens_go_to_its_active_pane_once_it_shows() {
        let mut streaming = kept_live("ab");

        let mut orders = reach(&streaming.seeing.ends);
        assert_eq!(request(&mut orders)["command"], "focus");
        say(&mut orders, &json!({"ok": {"tab": 11, "projection": 2}}));
        say(&mut streaming.surface, &tab_with_pane(11, 5, 2));
        streaming.seeing.rig.until("the other tab shows", |app| shown(app).as_deref() == Some("other"));
        let asked = request(&mut orders);
        assert_eq!((&asked["args"]["pane"], events(&asked)), (&json!(5), vec![key("a"), key("b")]));
    }

    #[test]
    fn keys_kept_for_a_tab_that_does_not_open_are_dropped_and_never_reach_another() {
        let mut streaming = kept_live("a");
        let mut orders = reach(&streaming.seeing.ends);
        assert_eq!(request(&mut orders)["command"], "focus");

        say(&mut orders, &json!({"error": {"code": "failed", "message": "no such tab"}}));

        streaming.seeing.rig.until("the refusal arrives", |app| app.watched().is_some_and(|w| w.typed.is_empty()));
        assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
        type_text(on(&mut streaming), "b");
        let asked = request(&mut orders);
        assert_eq!((&asked["args"]["pane"], events(&asked)), (&json!(1), vec![key("b")]));
    }

    #[test]
    fn keys_kept_for_a_tab_left_before_it_shows_are_dropped() {
        let mut streaming = kept_live("a");

        assert!(on(&mut streaming).select_remote("vps", RemoteItem::Tab(13)));

        assert_eq!(held(&mut streaming), []);
        assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
    }

    #[test]
    fn keys_kept_for_a_tab_that_never_shows_are_dropped_once_orders_would_be_late() {
        let mut streaming = kept_live("a");
        let now = Instant::now();
        assert!(on(&mut streaming).typed_wake(now).is_some_and(|wake| wake <= ANSWERS_WITHIN));

        on(&mut streaming).drop_late_typed(now);
        assert_eq!(held(&mut streaming), [key("a")]);
        on(&mut streaming).drop_late_typed(now + ANSWERS_WITHIN);

        assert_eq!(held(&mut streaming), []);
        assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
        assert_eq!(on(&mut streaming).typed_wake(now), None);
    }

    #[test]
    fn keys_kept_for_a_tab_are_dropped_by_a_click_on_a_pane_still_shown() {
        let mut streaming = streaming();
        split(&mut streaming);
        let mut streaming = kept(streaming, "a");
        let pane = on(&mut streaming).layout(AREA).pane;

        click(on(&mut streaming), Position::new(pane.x + 13, pane.y + 1));

        assert_eq!(held(&mut streaming), []);
        assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
    }

    #[test]
    fn keys_typed_after_a_lost_show_wait_for_that_tab() {
        let mut streaming = kept_live("a");

        lost(on(&mut streaming), Order::Show(11));
        type_text(on(&mut streaming), "b");

        assert_eq!(held(&mut streaming), [key("b")]);
        assert_eq!(toast(on(&mut streaming)), Some(UNREACHABLE));
    }

    #[test]
    fn keys_past_what_is_kept_are_dropped_and_a_toast_says_so() {
        let mut streaming = kept_live(&"a".repeat(1024));
        assert_eq!(toast(on(&mut streaming)), None);

        type_text(on(&mut streaming), "b");

        assert_eq!((held(&mut streaming).len(), toast(on(&mut streaming))), (1024, Some(UNREACHABLE)));
    }
}
