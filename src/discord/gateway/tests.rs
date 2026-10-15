use std::net::TcpListener;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;

use super::*;
use crate::messenger::Message as Said;
use crate::test_util::FakeHttp;

const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";
const WITHIN: Duration = Duration::from_secs(10);
const QUIET: u64 = 60_000;

enum Act {
    Send(Value),
    Close(u16),
}

struct FakeGateway {
    url: String,
    heard: Receiver<(usize, Value)>,
    acts: Sender<Act>,
    stop: Arc<AtomicBool>,
}

impl FakeGateway {
    fn start(heartbeat: u64) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a fake gateway");
        let url = format!("ws://{}", listener.local_addr().expect("its address"));
        let (heard_tx, heard) = mpsc::channel();
        let (acts, acts_rx) = mpsc::channel::<Act>();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                if stopped.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(stream) = stream else { continue };
                let Ok(mut socket) = tungstenite::accept(stream) else { continue };
                let _ = socket.get_ref().set_read_timeout(Some(Duration::from_millis(10)));
                let hello = json!({ "op": 10, "d": { "heartbeat_interval": heartbeat } });
                let _ = socket.send(Message::text(hello.to_string()));
                loop {
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            let _ = heard_tx.send((n, serde_json::from_str(text.as_str()).unwrap_or(Value::Null)));
                        }
                        Ok(Message::Close(frame)) => {
                            let code = frame.map(|frame| u16::from(frame.code));
                            let _ = heard_tx.send((n, json!({ "closed": code })));
                        }
                        Ok(_) => {}
                        Err(e) if waiting(&e) => {}
                        Err(_) => break,
                    }
                    while let Ok(act) = acts_rx.try_recv() {
                        let _ = match act {
                            Act::Send(payload) => socket.send(Message::text(payload.to_string())),
                            Act::Close(code) => socket.close(Some(CloseFrame { code: code.into(), reason: "".into() })),
                        };
                    }
                }
            }
        });
        Self { url, heard, acts, stop }
    }

    fn send(&self, payload: Value) {
        self.acts.send(Act::Send(payload)).expect("the fake gateway runs");
    }

    fn dispatch(&self, seq: i64, kind: &str, d: &Value) {
        self.send(json!({ "op": 0, "s": seq, "t": kind, "d": d }));
    }

    fn ready(&self) {
        self.dispatch(1, "READY", &json!({ "session_id": "s1", "resume_gateway_url": self.url }));
    }

    fn heard_where(&self, what: &str, pick: impl Fn(&Value) -> bool) -> (usize, Value) {
        let until = Instant::now() + WITHIN;
        loop {
            match self.heard.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok((n, payload)) if pick(&payload) => return (n, payload),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => panic!("the bot never sent {what}"),
            }
        }
    }

    fn heard(&self, op: u64) -> (usize, Value) {
        self.heard_where(&format!("op {op}"), |payload| payload["op"].as_u64() == Some(op))
    }

    fn close(&self, code: u16) {
        self.acts.send(Act::Close(code)).expect("the fake gateway runs");
    }
}

impl Drop for FakeGateway {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.url.trim_start_matches("ws://"));
    }
}

fn started(
    gateway: &FakeGateway,
    routes: Vec<(&'static str, u16, String)>,
) -> (FakeHttp, Link, Receiver<(Via, Event)>) {
    started_since(gateway, routes, 0)
}

fn started_since(
    gateway: &FakeGateway,
    routes: Vec<(&'static str, u16, String)>,
    since: i64,
) -> (FakeHttp, Link, Receiver<(Via, Event)>) {
    let mut all = vec![("GET /gateway/bot", 200, json!({ "url": gateway.url }).to_string())];
    all.extend(routes);
    let http = FakeHttp::start(all);
    let (tx, rx) = mpsc::channel();
    let link = start(3, &Bot::new(&http.url(), TOKEN), &tx, since);
    (http, link, rx)
}

fn greeted(gateway: &FakeGateway) -> (FakeHttp, Link, Receiver<(Via, Event)>) {
    let started = started(gateway, Vec::new());
    gateway.heard(2);
    gateway.ready();
    started
}

fn next(rx: &Receiver<(Via, Event)>, what: &str, pick: impl Fn(&Event) -> bool) -> Event {
    let until = Instant::now() + WITHIN;
    loop {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok((Via::Discord, event)) if pick(&event) => return event,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => panic!("no event: {what}"),
        }
    }
}

fn updates(rx: &Receiver<(Via, Event)>) -> Vec<Update> {
    match next(rx, "the updates", |e| matches!(e, Event::Updates(..))) {
        Event::Updates(3, updates) => updates,
        event => panic!("updates of the link: {event:?}"),
    }
}

fn said(rx: &Receiver<(Via, Event)>) -> (i64, i64, String) {
    match updates(rx).as_slice() {
        [Update::Message(Said { chat, user, text, .. })] => (*chat, *user, text.clone()),
        updates => panic!("a message: {updates:?}"),
    }
}

fn direct_message(seq: i64, gateway: &FakeGateway) {
    direct_message_at(seq, gateway, "55", "2026-10-15T10:00:00+00:00");
}

fn direct_message_at(seq: i64, gateway: &FakeGateway, id: &str, timestamp: &str) {
    gateway.dispatch(
        seq,
        "MESSAGE_CREATE",
        &json!({ "id": id, "channel_id": "1200", "content": "hi",
        "timestamp": timestamp, "author": { "id": "9", "username": "ada" } }),
    );
}

#[test]
fn the_bot_identifies_for_direct_messages() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, _rx) = started(&gateway, Vec::new());

    let (_, identify) = gateway.heard(2);

    assert_eq!((identify["d"]["token"].as_str(), identify["d"]["intents"].as_u64()), (Some(TOKEN), Some(4096)));
}

#[test]
fn heartbeats_go_on_while_they_are_acknowledged() {
    let gateway = FakeGateway::start(1000);
    let (_http, _link, _rx) = started(&gateway, Vec::new());

    let (first, _) = gateway.heard(1);
    gateway.send(json!({ "op": 11 }));
    let (second, _) = gateway.heard(1);
    gateway.send(json!({ "op": 11 }));
    let (third, _) = gateway.heard(1);

    assert_eq!((first, second, third), (0, 0, 0));
}

#[test]
fn a_heartbeat_asked_for_is_sent_at_once_on_the_same_connection() {
    let gateway = FakeGateway::start(3000);
    let (_http, _link, _rx) = started(&gateway, Vec::new());
    gateway.heard(1);

    gateway.send(json!({ "op": 1, "d": null }));
    let (connection, _) = gateway.heard(1);

    assert_eq!(connection, 0);
}

#[test]
fn a_heartbeat_left_unanswered_reconnects() {
    let gateway = FakeGateway::start(50);
    let (_http, _link, _rx) = started(&gateway, Vec::new());

    gateway.heard(1);
    let (again, _) = gateway.heard(2);

    assert_eq!(again, 1);
}

#[test]
fn a_direct_message_comes_in_as_an_update() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, rx) = greeted(&gateway);

    direct_message(2, &gateway);

    assert_eq!(said(&rx), (1200, 9, "hi".into()));
}

#[test]
fn a_button_press_is_acknowledged_at_once_and_comes_in() {
    let gateway = FakeGateway::start(QUIET);
    let (http, _link, rx) = started(&gateway, vec![("POST /interactions/77/tok/callback", 204, String::new())]);
    gateway.heard(2);

    gateway.dispatch(
        1,
        "INTERACTION_CREATE",
        &json!({ "id": "77", "token": "tok", "type": 3, "channel_id": "1200",
        "user": { "id": "9" }, "message": { "id": "55" }, "data": { "custom_id": "ab12:s:5" } }),
    );

    let updates = updates(&rx);
    assert!(matches!(updates.as_slice(), [Update::Press(press)] if press.id == "77:tok"), "{updates:?}");
    let acked = http.requests().into_iter().find(|r| r.starts_with("POST /interactions/77/tok/callback"));
    assert!(acked.is_some_and(|r| r.ends_with(r#"{"type":6}"#)));
}

#[test]
fn a_slash_command_comes_in_as_its_text_and_leaves_no_reply_behind() {
    let gateway = FakeGateway::start(QUIET);
    let (http, _link, rx) = started(
        &gateway,
        vec![
            ("POST /interactions/78/tok/callback", 204, String::new()),
            ("DELETE /webhooks/123456789/tok/messages/@original", 204, String::new()),
        ],
    );
    gateway.heard(2);

    gateway.dispatch(
        1,
        "INTERACTION_CREATE",
        &json!({ "id": "78", "token": "tok", "type": 2, "application_id": "123456789", "channel_id": "1200",
        "user": { "id": "9", "username": "ada" }, "data": { "name": "new", "type": 1 } }),
    );

    assert_eq!(said(&rx), (1200, 9, "/new".into()));
    crate::test_util::wait_until("the placeholder removed", || http.requests().len() == 3);
    let requests = http.requests();
    assert!(requests[1].starts_with("POST /interactions/78/tok/callback"), "{requests:?}");
    let acked = requests[1].split_once("\r\n\r\n").map(|(_, body)| serde_json::from_str::<Value>(body).ok());
    assert_eq!(acked.flatten(), Some(json!({ "type": 5, "data": { "flags": 64 } })));
    assert!(requests[2].starts_with("DELETE /webhooks/123456789/tok/messages/@original"), "{requests:?}");
}

#[test]
fn a_reconnect_request_resumes_the_session() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, rx) = greeted(&gateway);
    direct_message(2, &gateway);
    updates(&rx);

    gateway.send(json!({ "op": 7, "d": null }));

    let (connection, resume) = gateway.heard(6);
    assert_eq!(connection, 1);
    assert_eq!(resume["d"], json!({ "token": TOKEN, "session_id": "s1", "seq": 2 }));
}

#[test]
fn an_invalid_session_that_can_resume_resumes() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, _rx) = started(&gateway, Vec::new());
    gateway.heard(2);
    gateway.ready();

    gateway.send(json!({ "op": 9, "d": true }));

    let (connection, resume) = gateway.heard(6);
    assert_eq!((connection, resume["d"]["session_id"].as_str()), (1, Some("s1")));
}

#[rstest::rstest]
#[case::invalid_session(None)]
#[case::bad_sequence(Some(4007))]
#[case::session_timed_out(Some(4009))]
fn a_session_that_cannot_resume_identifies_anew(#[case] close: Option<u16>) {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, _rx) = started(&gateway, Vec::new());
    gateway.heard(2);
    gateway.ready();

    match close {
        Some(code) => gateway.close(code),
        None => gateway.send(json!({ "op": 9, "d": false })),
    }

    let (connection, _) = gateway.heard(2);
    assert_eq!(connection, 1);
}

#[test]
fn messages_from_before_the_start_are_dropped() {
    let gateway = FakeGateway::start(QUIET);
    let since = crate::time::parse_time("2026-10-15T10:00:00Z").expect("a time");
    let (_http, _link, rx) = started_since(&gateway, Vec::new(), since);
    gateway.heard(2);
    gateway.ready();

    direct_message_at(2, &gateway, "54", "2026-10-15T09:59:00+00:00");
    direct_message_at(3, &gateway, "55", "2026-10-15T10:00:01+00:00");

    let updates = updates(&rx);
    assert!(matches!(updates.as_slice(), [Update::Message(Said { id: 55, .. })]), "{updates:?}");
}

#[test]
fn reaching_the_gateway_again_clears_the_problem() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, rx) = started(&gateway, Vec::new());
    gateway.heard(2);

    gateway.close(4000);
    next(&rx, "the problem", |e| matches!(e, Event::Problem(_, Some(_))));
    let (connection, _) = gateway.heard(2);
    gateway.ready();

    assert_eq!(connection, 1);
    assert!(matches!(next(&rx, "the recovery", |e| matches!(e, Event::Problem(..))), Event::Problem(3, None)));
}

#[test]
fn dropping_the_link_closes_the_connection() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, link, _rx) = started(&gateway, Vec::new());
    gateway.heard(2);

    drop(link);

    let (_, closed) = gateway.heard_where("a close", |payload| payload.get("closed").is_some());
    assert_eq!(closed["closed"].as_u64(), Some(1000));
}

#[test]
fn a_refused_token_stops_the_link() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, rx) = started(&gateway, Vec::new());
    gateway.heard(2);

    gateway.close(4004);

    assert!(matches!(next(&rx, "the refusal", |e| matches!(e, Event::Rejected(_))), Event::Rejected(3)));
}

#[test]
fn a_refused_intent_is_a_problem_that_is_not_retried() {
    let gateway = FakeGateway::start(QUIET);
    let (_http, _link, rx) = started(&gateway, Vec::new());
    gateway.heard(2);

    gateway.close(4014);

    let problem = next(&rx, "the problem", |e| matches!(e, Event::Problem(_, Some(_))));
    assert!(matches!(problem, Event::Problem(3, Some(why)) if why.contains("4014")));
}
