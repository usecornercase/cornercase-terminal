use serde_json::{Value, json};

use super::*;
use crate::app::Surfaced;
use crate::app::surfaces::FRAME_EVERY;
use crate::control::surface::Frame;

const CLIENT: u64 = 9;

fn ask(p: &mut Popped, command: &str, args: &Value) -> Value {
    let text = json!({"contract": 1, "command": command, "args": args}).to_string();
    p.app.request(CLIENT, &text, Some(AREA), Instant::now());
    let answers = p.app.take_answers();
    assert_eq!(answers.len(), 1, "an answer at once");
    serde_json::from_str(&answers[0].1).expect("a reply")
}

fn subscribe(p: &mut Popped) -> Frame {
    let answer = ask(p, "surface", &json!({"popup": true}));
    serde_json::from_value(answer["ok"].clone()).expect("a frame")
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

fn last_shown(rendered: Vec<(u64, Surfaced)>) -> Option<String> {
    rendered.into_iter().last().map(|(client, surfaced)| {
        assert_eq!(client, CLIENT);
        let Surfaced::Frame(text) = surfaced else { panic!("the stream ended") };
        let value: Value = serde_json::from_str(&text).expect("a line");
        shown(&serde_json::from_value(value["ok"].clone()).expect("a frame"))
    })
}

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
        assert_eq!((popup.plugin.as_str(), popup.action.as_str()), (ID, "line"));
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
        let mut at = Instant::now();

        assert_eq!(typed(&mut p, &json!([{"text": "hello-popup"}]))["ok"]["pane"], json!(popup_id(&p)));

        let mut seen = String::new();
        wait_until("the popup's echo reaches a frame", || {
            drain(&mut p.app, &p.rx);
            at += FRAME_EVERY;
            if let Some(text) = last_shown(p.app.render_surfaces(&[CLIENT], at)) {
                seen = text;
            }
            seen.contains("hello-popup")
        });
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

    #[test]
    fn a_tab_or_a_size_with_it_is_malformed() {
        let mut p = popped();
        p.open("watch", None);

        let answer = ask(&mut p, "surface", &json!({"popup": true, "cols": 80, "rows": 24}));

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
        assert!(p.line().ends_with("\nhM-CM-)llo"), "{}", p.line());
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
    fn a_pane_with_it_is_malformed() {
        let mut p = popped();
        p.open("watch", None);

        let answer = ask(&mut p, "input", &json!({"popup": true, "pane": 1, "events": [{"text": "x"}]}));

        assert_eq!(code(&answer), "malformed");
    }
}
