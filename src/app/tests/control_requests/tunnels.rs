use std::time::Duration;

use super::*;
use crate::app::events::Streamed;
use crate::protocol::{Holding, Tunnel, TunnelOrder, TunnelState};

const WINDOW: u64 = 7;
const OBSERVER: u64 = 21;

fn app() -> (App, Receiver<AppEvent>) {
    empty_app()
}

fn holding(app: &App, tunnels: Vec<Tunnel>) -> Holding {
    Holding { browse: true, orders: app.opening.tunnels.sent(WINDOW), tunnels }
}

fn tunnel(port: u16, local: Option<u16>, state: TunnelState) -> Tunnel {
    Tunnel { port, local, state }
}

fn send(app: &mut App, command: &str, port: u16, now: Instant) {
    let text = json!({"contract": 1, "command": command, "args": {"port": port}}).to_string();
    app.request(CLIENT, &text, Some(AREA), now);
}

fn answer(app: &mut App) -> std::result::Result<Value, Refused> {
    let mut answers = app.take_answers();
    assert_eq!(answers.len(), 1, "one answer: {answers:?}");
    reply_of(&answers.remove(0).1)
}

fn attached(app: &mut App) {
    app.tunnels_reported(WINDOW, holding(app, vec![]));
}

fn watch_events(app: &mut App) {
    let text = r#"{"command":"events","args":{},"contract":1}"#;
    app.request(OBSERVER, text, Some(AREA), Instant::now());
    app.take_answers();
}

fn event_lines(app: &mut App) -> Vec<Value> {
    app.take_events()
        .into_iter()
        .filter(|(client, _)| *client == OBSERVER)
        .filter_map(|(_, streamed)| match streamed {
            Streamed::Event(text) => Some(serde_json::from_str::<Value>(&text).expect("a line")["ok"].clone()),
            Streamed::Snapshot(_) | Streamed::End => None,
        })
        .collect()
}

mod listing {
    use super::*;

    #[test]
    fn says_there_are_no_windows_when_none_holds_tunnels() {
        let (mut app, _rx) = app();

        let list = call(&mut app, "tunnels", &json!({})).expect("a list");

        assert_eq!(list, json!({"tunnels": [], "windows": 0}));
    }

    #[test]
    fn lists_every_holders_tunnels_by_port() {
        let (mut app, _rx) = app();
        app.tunnels_reported(
            WINDOW,
            holding(
                &app,
                vec![
                    tunnel(8080, None, TunnelState::Down("refused".into())),
                    tunnel(5173, Some(5174), TunnelState::Up),
                ],
            ),
        );

        let list = call(&mut app, "tunnels", &json!({})).expect("a list");

        assert_eq!(
            list,
            json!({"tunnels": [
                {"port": 5173, "local": 5174, "state": "up", "error": null, "holder": WINDOW},
                {"port": 8080, "local": null, "state": "down", "error": "refused", "holder": WINDOW},
            ], "windows": 1})
        );
    }

    #[test]
    fn status_carries_them_only_when_there_are_some() {
        let (mut app, _rx) = app();
        let before = call(&mut app, "status", &json!({})).expect("a report");
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Up)]));

        let after = call(&mut app, "status", &json!({})).expect("a report");

        assert_eq!((before.get("tunnels"), after["tunnels"][0]["port"].clone()), (None, json!(5173)));
    }
}

mod opening {
    use super::*;

    #[test]
    fn is_unavailable_when_no_window_holds_tunnels() {
        let (mut app, _rx) = app();

        send(&mut app, "tunnel-open", 5173, Instant::now());

        let refused = answer(&mut app).expect_err("refused");
        assert_eq!(refused.code, "unavailable");
        assert!(refused.message.contains("cornercase remote"), "{}", refused.message);
    }

    #[test]
    fn port_zero_is_malformed() {
        let (mut app, _rx) = app();
        attached(&mut app);

        send(&mut app, "tunnel-open", 0, Instant::now());

        assert_eq!(answer(&mut app).expect_err("refused").code, "malformed");
    }

    #[test]
    fn waits_for_the_window_then_answers_with_the_tunnel() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();

        send(&mut app, "tunnel-open", 5173, now);
        app.check_requests(now);

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Open(5173))]);
        assert_eq!(app.take_answers(), Vec::new());
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5174), TunnelState::Opening)]));
        app.check_requests(now);
        assert_eq!(app.take_answers(), Vec::new());
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5174), TunnelState::Up)]));
        app.check_requests(now);
        let list = answer(&mut app).expect("the tunnel");
        assert_eq!(list["tunnels"][0]["local"], json!(5174));
    }

    #[test]
    fn a_report_sent_before_the_window_got_the_order_does_not_answer_it() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 3000, now);
        send(&mut app, "tunnel-open", 5173, now);
        app.take_answers();

        let first_only = Holding { browse: true, orders: 1, tunnels: vec![tunnel(3000, Some(3000), TunnelState::Up)] };
        app.tunnels_reported(WINDOW, first_only);
        app.check_requests(now);

        let answers = app.take_answers();
        assert_eq!(answers.len(), 1, "{answers:?}");
        assert_eq!(reply_of(&answers[0].1).expect("the first")["tunnels"][0]["port"], json!(3000));
    }

    #[test]
    fn fails_with_the_reason_ssh_gave() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.tunnels_reported(
            WINDOW,
            holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Down("refused".into()))]),
        );
        app.check_requests(now);

        let refused = answer(&mut app).expect_err("failed");
        assert_eq!(refused.message, "could not tunnel port 5173: refused");
    }

    #[test]
    fn is_answered_when_the_window_leaves() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.forget(WINDOW);
        app.check_requests(now);

        let refused = answer(&mut app).expect_err("failed");
        assert_eq!(refused.message, "could not tunnel port 5173: no window answered");
    }

    #[test]
    fn times_out_when_no_window_reports() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.check_requests(now + Duration::from_secs(46));

        let refused = answer(&mut app).expect_err("timed out");
        assert!(refused.message.starts_with("timed out after 45s: the windows have not said"), "{}", refused.message);
    }
}

mod closing {
    use super::*;

    #[test]
    fn an_unknown_port_is_not_found() {
        let (mut app, _rx) = app();
        attached(&mut app);

        send(&mut app, "tunnel-close", 5173, Instant::now());

        assert_eq!(answer(&mut app).expect_err("refused").code, "not-found");
    }

    #[test]
    fn orders_every_holder_and_answers_once_it_is_gone() {
        let (mut app, _rx) = app();
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Up)]));
        let now = Instant::now();

        send(&mut app, "tunnel-close", 5173, now);
        app.check_requests(now);

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Close(5173))]);
        assert_eq!(app.take_answers(), Vec::new());
        app.tunnels_reported(WINDOW, holding(&app, vec![]));
        app.check_requests(now);
        assert_eq!(answer(&mut app).expect("closed")["tunnels"], json!([]));
    }
}

mod pending_orders {
    use super::*;

    #[test]
    fn closing_a_port_whose_open_is_still_on_its_way_orders_the_close_too() {
        let (mut app, _rx) = app();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);
        app.take_tunnel_orders();

        send(&mut app, "tunnel-close", 5173, now);

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Close(5173))]);
    }

    #[test]
    fn a_reconnected_window_starts_counting_from_the_orders_it_reports() {
        let (mut app, _rx) = app();
        app.tunnels_reported(WINDOW, Holding { browse: true, orders: 5, tunnels: vec![] });
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.check_requests(now);

        assert_eq!(app.take_answers().len(), 0);
    }
}

mod events {
    use super::*;

    #[test]
    fn a_tunnel_coming_up_and_its_window_leaving_are_published() {
        let (mut app, _rx) = app();
        watch_events(&mut app);

        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5174), TunnelState::Up)]));
        app.forget(WINDOW);

        let lines = event_lines(&mut app);
        let states: Vec<&Value> = lines.iter().map(|line| &line["state"]).collect();
        assert_eq!(states, [&json!("up"), &json!("gone")], "{lines:?}");
        assert_eq!(lines[0]["event"], json!("tunnel"));
    }

    #[test]
    fn a_reader_of_one_pane_gets_none() {
        let (mut app, _rx) = app();
        let text = r#"{"command":"events","args":{"panes":[1]},"contract":1}"#;
        app.request(OBSERVER, text, Some(AREA), Instant::now());
        app.take_answers();

        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5174), TunnelState::Up)]));

        assert_eq!(event_lines(&mut app), Vec::<Value>::new());
    }
}
