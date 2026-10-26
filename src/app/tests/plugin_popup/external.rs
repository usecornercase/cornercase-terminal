use rstest::rstest;
use serde_json::{Value, json};

use super::*;
use crate::app::Surfaced;
use crate::app::surfaces::FRAME_EVERY;
use crate::control::surface::Frame;

const CLIENT: u64 = 9;

fn ask(p: &mut Popped, command: &str, args: &Value) -> Value {
    ask_with(p, &json!({"contract": 1, "command": command, "args": args}))
}

fn ask_with(p: &mut Popped, request: &Value) -> Value {
    let text = request.to_string();
    p.app.request(CLIENT, &text, Some(AREA), Instant::now());
    let answers = p.app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    serde_json::from_str(&answers[0].1).expect("a reply")
}

fn subscribe(p: &mut Popped) -> Frame {
    let answer = ask(p, "surface", &json!({"popup": true}));
    serde_json::from_value(answer["ok"].clone()).expect("a frame")
}

fn printed(p: &mut Popped, text: &str) {
    assert!(typed(p, &json!([{"text": text}]))["ok"].is_object());
    wait_until("the popup's program echoes", || {
        drain(&mut p.app, &p.rx);
        p.app.popup().is_some_and(|popup| popup.term.emulator.screen_text().is_ok_and(|screen| screen.contains(text)))
    });
}

fn lines_at(p: &mut Popped, at: Instant) -> Vec<Value> {
    p.app
        .render_surfaces(&[CLIENT], at)
        .into_iter()
        .map(|(client, surfaced)| {
            assert_eq!(client, CLIENT);
            let Surfaced::Frame(text) = surfaced else { panic!("the stream ended") };
            let value: Value = serde_json::from_str(&text).expect("a line");
            value["ok"].clone()
        })
        .collect()
}

fn typed(p: &mut Popped, events: &Value) -> Value {
    ask(p, "input", &json!({"popup": true, "events": events}))
}

fn code(answer: &Value) -> &str {
    answer["error"]["code"].as_str().unwrap_or_else(|| panic!("not refused: {answer}"))
}

fn shown(frame: &Frame) -> String {
    frame.content.panes[0].lines.iter().flatten().flat_map(|run| run.cells.iter().map(String::as_str)).collect()
}

const E_ACUTE_SHOWN: [&str; 2] = ["M-CM-)", "é"];

fn popup_id(p: &Popped) -> u64 {
    p.app.popup().expect("a popup").term.id
}

mod surface {
    use super::*;

    #[test]
    fn streams_the_popups_terminal_as_one_pane_at_its_own_size() {
        let mut p = popped();
        p.open("line", None);

        let frame = subscribe(&mut p);

        let popup = frame.popup.as_ref().expect("the popup");
        assert_eq!((popup.pane, popup.plugin.as_str(), popup.action.as_str()), (popup_id(&p), ID, "line"));
        assert_eq!((frame.tab, frame.cols, frame.rows), (0, 60, 10));
        assert!(frame.content.layout.is_none());
        assert_eq!(frame.content.panes.len(), 1);
        let pane = &frame.content.panes[0];
        assert_eq!((pane.pane, pane.active, pane.size.cols, pane.size.rows), (popup_id(&p), true, 60, 10));
    }

    #[test]
    fn what_its_program_prints_comes_in_the_next_frame() {
        let mut p = popped();
        p.open("line", None);
        subscribe(&mut p);
        let next = Instant::now() + FRAME_EVERY;

        printed(&mut p, "hello-popup");

        let lines = lines_at(&mut p, next);
        assert_eq!(lines.len(), 1, "one frame, before the periodic refresh");
        let frame: Frame = serde_json::from_value(lines[0].clone()).expect("a frame");
        assert!(shown(&frame).contains("hello-popup"), "{}", shown(&frame));
    }

    #[test]
    fn a_redraw_sends_a_complete_frame_at_once() {
        let mut p = popped();
        p.open("watch", None);
        let first = subscribe(&mut p);

        let answer = ask(&mut p, "redraw", &json!({"surface": first.surface}));
        let at = Instant::now();

        assert_eq!((answer["ok"]["surface"].as_u64(), answer["ok"]["tab"].as_u64()), (Some(first.surface), Some(0)));
        let lines = lines_at(&mut p, at);
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0]["revision"].as_u64(), lines[0]["base"].is_null()), (Some(2), true));
        assert_eq!(lines[0]["popup"]["pane"].as_u64(), Some(popup_id(&p)));
    }

    #[test]
    fn with_patches_what_changed_comes_as_a_patch_naming_the_popup() {
        let mut p = popped();
        p.open("line", None);
        ask(&mut p, "surface", &json!({"popup": true, "patches": true}));
        let next = Instant::now() + FRAME_EVERY;

        printed(&mut p, "patched");

        let lines = lines_at(&mut p, next);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["base"].as_u64(), Some(1));
        assert_eq!(lines[0]["popup"]["action"], json!("line"));
        assert_eq!(lines[0]["panes"][0]["pane"].as_u64(), Some(popup_id(&p)));
    }

    #[test]
    fn a_new_window_size_sends_a_complete_frame_at_the_popups_new_size() {
        let mut p = popped();
        p.open("line", None);
        subscribe(&mut p);
        let next = Instant::now() + FRAME_EVERY;

        p.app.resize(Rect::new(0, 0, 50, 12));
        p.app.touch_surfaces();

        let lines = lines_at(&mut p, next);
        assert_eq!(lines.len(), 1);
        assert_eq!((lines[0]["cols"].as_u64(), lines[0]["rows"].as_u64()), (Some(38), Some(7)));
        assert!(lines[0]["base"].is_null());
    }

    #[test]
    fn closing_the_popup_ends_the_stream() {
        let mut p = popped();
        p.open("watch", None);
        subscribe(&mut p);

        p.app.close_plugin_popup();

        assert_eq!(p.app.render_surfaces(&[CLIENT], Instant::now()), [(CLIENT, Surfaced::End)]);
        assert!(!p.app.streams_surfaces());
    }

    #[test]
    fn a_new_popup_is_not_streamed_by_the_old_stream() {
        let mut p = popped();
        p.open("watch", None);
        subscribe(&mut p);
        p.app.close_plugin_popup();
        p.open("line", None);

        assert_eq!(p.app.render_surfaces(&[CLIENT], Instant::now()), [(CLIENT, Surfaced::End)]);
    }

    #[test]
    fn without_a_popup_it_is_not_found() {
        let mut p = popped();

        assert_eq!(code(&ask(&mut p, "surface", &json!({"popup": true}))), "not-found");
    }

    #[rstest]
    #[case::a_tab(json!({"contract": 1, "command": "surface", "args": {"popup": true, "tab": 1}}))]
    #[case::a_size(json!({"contract": 1, "command": "surface", "args": {"popup": true, "cols": 80, "rows": 24}}))]
    fn a_tab_or_a_size_with_it_is_malformed(#[case] request: Value) {
        let mut p = popped();
        p.open("watch", None);

        let answer = ask_with(&mut p, &request);

        assert_eq!(code(&answer), "malformed");
    }

    #[test]
    fn a_view_with_it_is_malformed() {
        let mut p = popped();
        p.open("watch", None);
        let open = json!({"command": "view", "args": {"cols": 80, "rows": 24}}).to_string();
        p.app.request(CLIENT + 1, &open, Some(AREA), Instant::now());
        let answers = p.app.take_answers();
        let opened: Value = serde_json::from_str(&answers[0].1).expect("a reply");
        let view = opened["ok"]["view"]["id"].as_u64().expect("a view");

        let answer =
            ask_with(&mut p, &json!({"contract": 1, "view": view, "command": "surface", "args": {"popup": true}}));

        assert_eq!(code(&answer), "malformed");
    }

    #[test]
    fn it_cannot_be_resized() {
        let mut p = popped();
        p.open("watch", None);
        let frame = subscribe(&mut p);

        let answer = ask(&mut p, "resize", &json!({"surface": frame.surface, "cols": 80, "rows": 24}));

        assert_eq!(code(&answer), "failed");
    }
}

mod input {
    use super::*;

    #[test]
    fn keys_and_text_reach_its_program() {
        let mut p = popped();
        p.open("line", None);

        let answer = typed(&mut p, &json!([{"text": "héllo"}, {"key": {"code": "enter"}}]));

        assert!(answer["ok"]["bytes"].as_u64().is_some_and(|bytes| bytes > 0), "{answer}");
        let line = p.line();
        assert!(E_ACUTE_SHOWN.iter().any(|shown| line.ends_with(&format!("\nh{shown}llo"))), "{line}");
    }

    #[test]
    fn a_click_reaches_a_program_that_asked_for_the_mouse() {
        let mut p = popped();
        p.open("line", None);
        p.wait_mouse();

        let answer = typed(&mut p, &json!([{"mouse": {"kind": "down", "x": 2, "y": 1}}, {"key": {"code": "enter"}}]));

        assert_eq!(answer["ok"]["skipped"], json!([]));
        assert!(p.line().ends_with("\n^[[<0;3;2M"), "{}", p.line());
    }

    #[test]
    fn a_cell_outside_its_terminal_sends_nothing() {
        let mut p = popped();
        p.open("line", None);

        let answer = typed(&mut p, &json!([{"text": "x"}, {"mouse": {"kind": "down", "x": 60, "y": 0}}]));

        assert_eq!(code(&answer), "conflict");
    }

    #[test]
    fn without_a_popup_it_is_not_found() {
        let mut p = popped();

        assert_eq!(code(&typed(&mut p, &json!([{"text": "x"}]))), "not-found");
    }

    #[test]
    fn its_own_id_names_it() {
        let mut p = popped();
        p.open("line", None);
        let id = popup_id(&p);

        let answer = ask(
            &mut p,
            "input",
            &json!({"popup": true, "pane": id, "events": [{"text": "mine"}, {"key": {"code": "enter"}}]}),
        );

        assert_eq!(answer["ok"]["pane"].as_u64(), Some(id));
        assert!(p.line().ends_with("\nmine"), "{}", p.line());
    }

    #[test]
    fn the_id_of_a_closed_popup_reaches_nothing_in_the_next_one() {
        let mut p = popped();
        p.open("watch", None);
        let gone = popup_id(&p);
        p.app.close_plugin_popup();
        p.open("line", None);

        let answer = ask(
            &mut p,
            "input",
            &json!({"popup": true, "pane": gone, "events": [{"text": "secret"}, {"key": {"code": "enter"}}]}),
        );

        assert_eq!(code(&answer), "not-found");
        assert_eq!(
            answer["error"]["message"],
            json!(format!(
                "the open popup is not {gone}, so nothing was sent; read `popup.pane` from its surface lines"
            ))
        );
        typed(&mut p, &json!([{"text": "next"}, {"key": {"code": "enter"}}]));
        assert!(p.line().ends_with("\nnext"), "{}", p.line());
    }
}
