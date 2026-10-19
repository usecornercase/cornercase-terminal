use serde_json::Value;

use super::super::tests::a_bug::Attached;
use super::*;
use crate::app::FRAME_EVERY;
use crate::protocol::ClientMessage;
use crate::server::{ServerEvent, Step};
use crate::test_util::wait_until;

const STREAMER: u64 = 1;

fn ask(attached: &mut Attached, request: &str) -> Vec<ServerMessage> {
    let event = ServerEvent::Message(STREAMER, ClientMessage::Request(request.into()));
    let _ = attached.server.step(Step::Event(event));
    let _ = attached.server.step(Step::Answer);
    let mut got = Vec::new();
    wait_until("the answer arrives", || {
        got.extend(attached.messages());
        !got.is_empty()
    });
    got
}

fn json(message: &ServerMessage) -> Value {
    match message {
        ServerMessage::Response(text) => serde_json::from_str(text).expect("JSON"),
        other => panic!("not an answer: {other:?}"),
    }
}

fn tab_and_pane(attached: &mut Attached) -> (u64, u64) {
    let answer = json(&ask(attached, r#"{"command":"status","args":{}}"#)[0]);
    let tab = &answer["ok"]["projects"][0]["workspaces"][0]["tabs"][0];
    (tab["id"].as_u64().expect("a tab"), tab["panes"][0]["id"].as_u64().expect("a pane"))
}

fn hidden_tab(attached: &mut Attached) -> (u64, u64) {
    let answer = json(&ask(attached, r#"{"command":"new-tab","args":{}}"#)[0]);
    let ids = &answer["ok"];
    (ids["tab"].as_u64().expect("a tab"), ids["pane"].as_u64().expect("a pane"))
}

fn surface(attached: &mut Attached, tab: u64) -> Value {
    let request = format!(r#"{{"command":"surface","args":{{"tab":{tab},"cols":80,"rows":24}}}}"#);
    json(&ask(attached, &request)[0])
}

fn revisions(messages: &[ServerMessage]) -> Vec<u64> {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::Response(text) => serde_json::from_str::<Value>(text).ok()?["ok"]["revision"].as_u64(),
            _ => None,
        })
        .collect()
}

fn until_frame(attached: &mut Attached, revision: u64) -> Vec<ServerMessage> {
    let mut got = Vec::new();
    wait_until("the frame arrives", || {
        attached.server.surfaces();
        got.extend(attached.messages());
        revisions(&got).contains(&revision)
    });
    got
}

fn split(attached: &mut Attached, pane: u64) {
    let request = format!(r#"{{"command":"split","args":{{"pane":{pane}}}}}"#);
    ask(attached, &request);
}

#[test]
fn the_first_frame_goes_out_before_any_later_one() {
    let mut attached = Attached::new();
    let (tab, pane) = tab_and_pane(&mut attached);

    let first = surface(&mut attached, tab);
    split(&mut attached, pane);
    let later = until_frame(&mut attached, 2);

    assert_eq!(first["ok"]["revision"], 1);
    assert_eq!(revisions(&later), [2]);
}

#[test]
fn a_reader_with_frames_queued_gets_none_until_it_has_room() {
    let mut attached = Attached::new();
    let (tab, pane) = tab_and_pane(&mut attached);
    surface(&mut attached, tab);
    attached.server.clients[0].out.queued.store(SURFACES_QUEUED, std::sync::atomic::Ordering::Relaxed);

    split(&mut attached, pane);
    attached.server.app.touch_surfaces();
    let ready = attached.server.with_room();
    attached.server.app.render_surfaces(&ready, Instant::now() + FRAME_EVERY);
    let blocked_ids = attached.server.with_room();
    let blocked = attached.messages();
    attached.server.clients[0].out.queued.store(0, std::sync::atomic::Ordering::Relaxed);
    let open_ids = attached.server.with_room();
    let got = until_frame(&mut attached, 2);

    assert_eq!((blocked_ids, open_ids), (vec![], vec![STREAMER]));
    assert_eq!(revisions(&blocked), Vec::<u64>::new());
    assert_eq!(revisions(&got), [2]);
}

#[test]
fn a_reader_with_frames_queued_gets_a_complete_frame_once_it_has_room() {
    let mut attached = Attached::new();
    let (tab, pane) = tab_and_pane(&mut attached);
    let request = format!(r#"{{"command":"surface","args":{{"tab":{tab},"cols":80,"rows":24,"patches":true}}}}"#);
    json(&ask(&mut attached, &request)[0]);
    attached.server.clients[0].out.queued.store(SURFACES_QUEUED, std::sync::atomic::Ordering::Relaxed);

    attached
        .server
        .app
        .handle_event(crate::app::AppEvent::Output(pane, b"held".to_vec()), ratatui::layout::Rect::new(0, 0, 80, 24))
        .expect("output");
    let ready = attached.server.with_room();
    attached.server.app.render_surfaces(&ready, Instant::now() + FRAME_EVERY);
    attached.server.clients[0].out.queued.store(0, std::sync::atomic::Ordering::Relaxed);
    let got = until_frame(&mut attached, 2);

    let frame = got
        .iter()
        .find_map(|m| match m {
            ServerMessage::Response(text) => {
                serde_json::from_str::<Value>(text).ok().filter(|v| v["ok"]["revision"] == 2)
            }
            _ => None,
        })
        .expect("the frame");
    assert!(frame["ok"].get("base").is_none(), "{frame}");
    assert!(frame["ok"]["panes"][0].get("rect").is_some());
}

#[test]
fn output_only_a_surface_shows_sends_a_frame_and_draws_no_window() {
    let mut attached = Attached::new();
    let (tab, pane) = hidden_tab(&mut attached);
    surface(&mut attached, tab);
    let _ = attached.server.step(Step::Draw);

    let _ = attached.server.handle(ServerEvent::App(crate::app::AppEvent::Output(pane, b"hidden-text".to_vec())));

    assert!(!attached.server.due(Instant::now()));
    let got = until_frame(&mut attached, 2);
    let spelled = r#""h","i","d","d","e","n","-","t","e","x","t""#;
    assert!(got.iter().any(|m| matches!(m, ServerMessage::Response(text) if text.contains(spelled))));
}

#[test]
fn a_stream_ends_with_detached_when_its_tab_closes() {
    let mut attached = Attached::new();
    let (tab, _) = hidden_tab(&mut attached);
    surface(&mut attached, tab);

    let close = format!(r#"{{"command":"close","args":{{"item":{{"tab":{tab}}}}}}}"#);
    ask(&mut attached, &close);
    let mut got = Vec::new();
    wait_until("the stream ends", || {
        let _ = attached.server.step(Step::Refresh);
        attached.server.surfaces();
        got.extend(attached.messages());
        got.iter().any(|m| matches!(m, ServerMessage::Detached))
    });

    assert!(!attached.server.app.streams_surfaces());
}

#[test]
fn using_the_window_takes_back_the_tab_a_view_sizes() {
    let mut attached = Attached::new();
    let opened = json(&ask(&mut attached, r#"{"command":"view","args":{"cols":50,"rows":10}}"#)[0]);
    let controller = |attached: &mut Attached| {
        let status = json(&ask(attached, r#"{"command":"status","args":{}}"#)[0]);
        status["ok"]["projects"][0]["workspaces"][0]["tabs"][0]["controller"].clone()
    };
    let view = opened["ok"]["view"]["id"].clone();
    let sized = controller(&mut attached);

    let key =
        crossterm::event::KeyEvent::new(crossterm::event::KeyCode::Char('a'), crossterm::event::KeyModifiers::NONE);
    let typed = ServerEvent::Message(STREAMER, ClientMessage::Event(crossterm::event::Event::Key(key)));
    let _ = attached.server.step(Step::Event(typed));
    let taken = controller(&mut attached);

    assert_eq!((opened["ok"]["view"]["controls"].clone(), sized), (Value::Bool(true), view));
    assert_eq!(taken, Value::Null);
}
