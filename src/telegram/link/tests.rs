use std::sync::mpsc::RecvTimeoutError;

use super::*;
use crate::telegram::Message;
use crate::test_util::FakeHttp;

const WITHIN: Duration = Duration::from_secs(10);
const UPDATES: &str = "POST /bot1:t/getUpdates";
const SEND: &str = "POST /bot1:t/sendMessage";

fn started(server: &FakeHttp, since: i64) -> (Link, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let link = Link::start(3, &Bot::new(&server.url(), "1:t"), &tx, Duration::ZERO, since);
    (link, rx)
}

fn next(rx: &Receiver<Event>, what: &str, pick: impl Fn(&Event) -> bool) -> Event {
    let until = Instant::now() + WITHIN;
    loop {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(event) if pick(&event) => return event,
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => panic!("no event: {what}"),
        }
    }
}

fn message(id: i64, date: i64) -> String {
    format!(
        r#"{{"update_id":{id},"message":{{"message_id":{id},"date":{date},"text":"hi","chat":{{"id":4,"type":"private"}},"from":{{"id":9}}}}}}"#
    )
}

fn send(tag: Option<u64>) -> Order {
    Order::Send { chat: 4, html: "<b>hi</b>".into(), keyboard: Vec::new(), reply_to: None, ask: None, tag }
}

#[test]
fn new_messages_come_in_and_older_ones_are_dropped() {
    let answer = format!(r#"{{"ok":true,"result":[{},{}]}}"#, message(10, 100), message(11, 900));
    let server = FakeHttp::start(vec![(UPDATES, 200, answer), (UPDATES, 200, r#"{"ok":true,"result":[]}"#.into())]);

    let (_link, rx) = started(&server, 500);

    let Event::Updates(link, updates) = next(&rx, "the updates", |e| matches!(e, Event::Updates(..))) else {
        unreachable!()
    };
    let ids: Vec<i64> = updates
        .iter()
        .filter_map(|u| if let Update::Message(Message { id, .. }) = u { Some(*id) } else { None })
        .collect();
    assert_eq!((link, ids), (3, vec![11]));
    let offset = |request: &str| request.contains(r#""offset":12"#);
    crate::test_util::wait_until("the next poll moves past them", || server.requests().iter().any(|r| offset(r)));
}

#[test]
fn a_refused_token_stops_the_link() {
    let server = FakeHttp::start(vec![(UPDATES, 401, r#"{"ok":false,"error_code":401}"#)]);

    let (_link, rx) = started(&server, 0);

    assert!(matches!(next(&rx, "the refusal", |e| matches!(e, Event::Rejected(_))), Event::Rejected(3)));
    thread::sleep(Duration::from_millis(200));
    assert_eq!(server.requests().iter().filter(|r| r.starts_with(UPDATES)).count(), 1);
}

#[test]
fn a_sent_message_reports_its_id_after_waiting_out_a_limit() {
    let server = FakeHttp::start(vec![
        (UPDATES, 200, r#"{"ok":true,"result":[]}"#),
        (SEND, 429, r#"{"ok":false,"error_code":429,"parameters":{"retry_after":0}}"#),
        (SEND, 200, r#"{"ok":true,"result":{"message_id":77}}"#),
    ]);
    let (link, rx) = started(&server, 0);

    link.send(send(None));
    link.send(send(Some(5)));

    let sent = next(&rx, "the message id", |e| matches!(e, Event::Sent { .. }));
    assert!(matches!(sent, Event::Sent { link: 3, tag: 5, message: 77 }), "{sent:?}");
}

#[test]
fn a_message_telegram_cannot_parse_goes_again_as_plain_text() {
    let server = FakeHttp::start(vec![
        (UPDATES, 200, r#"{"ok":true,"result":[]}"#),
        (SEND, 400, r#"{"ok":false,"error_code":400,"description":"Bad Request: can't parse entities"}"#),
        (SEND, 200, r#"{"ok":true,"result":{"message_id":8}}"#),
    ]);
    let (link, rx) = started(&server, 0);

    link.send(send(Some(1)));

    next(&rx, "the message id", |e| matches!(e, Event::Sent { .. }));
    let sends: Vec<String> = server.requests().into_iter().filter(|r| r.starts_with(SEND)).collect();
    assert!(sends[0].contains(r#""parse_mode":"HTML""#) && !sends[1].contains("parse_mode"), "{sends:?}");
    assert!(sends[1].contains(r#""text":"hi""#), "{sends:?}");
}
