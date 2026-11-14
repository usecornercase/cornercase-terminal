use std::time::Duration;

use rstest::rstest;

use super::*;
use crate::app::events::Streamed;
use crate::protocol::{Holding, Tunnel, TunnelOrder, TunnelState};

const WINDOW: u64 = 7;
const SECOND_WINDOW: u64 = 8;
const OBSERVER: u64 = 21;
const HOLD: u64 = 33;
const TAKER: u64 = 45;
const OTHER_TAKER: u64 = 46;
const REPORTER: u64 = 47;

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
    lines_for(app, OBSERVER)
}

fn lines_for(app: &mut App, client: u64) -> Vec<Value> {
    app.take_events()
        .into_iter()
        .filter(|(from, _)| *from == client)
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

        assert_eq!(list, json!({"tunnels": [], "windows": 0, "takers": 0}));
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
                {"port": 5173, "local": 5174, "state": "up", "error": null, "holder": WINDOW, "kind": "window"},
                {"port": 8080, "local": null, "state": "down", "error": "refused", "holder": WINDOW, "kind": "window"},
            ], "windows": 1, "takers": 0})
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
    fn waits_for_every_window_before_answering() {
        let (mut app, _rx) = app();
        attached(&mut app);
        app.tunnels_reported(SECOND_WINDOW, Holding { browse: true, orders: 0, tunnels: vec![] });
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);
        app.take_tunnel_orders();

        let up = vec![tunnel(5173, Some(5173), TunnelState::Up)];
        app.tunnels_reported(WINDOW, holding(&app, up.clone()));
        app.check_requests(now);
        assert_eq!(app.take_answers(), Vec::new());
        let second = Holding { browse: true, orders: app.opening.tunnels.sent(SECOND_WINDOW), tunnels: up };
        app.tunnels_reported(SECOND_WINDOW, second);
        app.check_requests(now);

        let list = answer(&mut app).expect("both up");
        assert_eq!(list["tunnels"].as_array().map(Vec::len), Some(2));
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
    fn answers_with_every_tunnel_not_only_the_closed_port() {
        let (mut app, _rx) = app();
        let both = vec![tunnel(3000, Some(3000), TunnelState::Up), tunnel(5173, Some(5173), TunnelState::Up)];
        app.tunnels_reported(WINDOW, holding(&app, both));
        let now = Instant::now();
        send(&mut app, "tunnel-close", 5173, now);
        app.take_tunnel_orders();

        let after = vec![tunnel(3000, Some(3000), TunnelState::Up), tunnel(5173, Some(5173), TunnelState::Closed)];
        app.tunnels_reported(WINDOW, holding(&app, after));
        app.check_requests(now);

        let ports: Vec<Value> = answer(&mut app).expect("closed")["tunnels"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|tunnel| tunnel["port"].clone())
            .collect();
        assert_eq!(ports, [json!(3000), json!(5173)]);
    }

    #[test]
    fn an_unknown_port_is_not_found() {
        let (mut app, _rx) = app();
        attached(&mut app);

        send(&mut app, "tunnel-close", 5173, Instant::now());

        assert_eq!(answer(&mut app).expect_err("refused").code, "not-found");
    }

    #[test]
    fn orders_every_holder_and_answers_once_it_is_closed() {
        let (mut app, _rx) = app();
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Up)]));
        let now = Instant::now();

        send(&mut app, "tunnel-close", 5173, now);
        app.check_requests(now);

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Close(5173))]);
        assert_eq!(app.take_answers(), Vec::new());
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Closed)]));
        app.check_requests(now);
        let list = answer(&mut app).expect("closed");
        assert_eq!(list["tunnels"][0]["port"], json!(5173));
        assert_eq!(list["tunnels"][0]["state"], json!("closed"));
    }
}

mod forgetting {
    use super::*;

    #[test]
    fn an_unknown_port_is_not_found_and_port_zero_is_malformed() {
        let (mut app, _rx) = app();
        attached(&mut app);

        send(&mut app, "tunnel-forget", 5173, Instant::now());
        assert_eq!(answer(&mut app).expect_err("refused").code, "not-found");

        send(&mut app, "tunnel-forget", 0, Instant::now());
        assert_eq!(answer(&mut app).expect_err("refused").code, "malformed");
    }

    #[test]
    fn orders_every_holder_and_answers_once_the_port_is_no_longer_listed() {
        let (mut app, _rx) = app();
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Closed)]));
        let now = Instant::now();

        send(&mut app, "tunnel-forget", 5173, now);
        app.check_requests(now);

        assert_eq!(app.take_tunnel_orders(), [(WINDOW, TunnelOrder::Forget(5173))]);
        assert_eq!(app.take_answers(), Vec::new());
        app.tunnels_reported(WINDOW, holding(&app, vec![]));
        app.check_requests(now);
        assert_eq!(answer(&mut app).expect("forgotten")["tunnels"], json!([]));
    }
}

mod holding_for_a_program {
    use super::*;

    fn hold(app: &mut App, client: u64, port: u16, local: u16) {
        let text = json!({"contract": 1, "command": "tunnel-hold", "args": {"port": port, "local": local}});
        app.request(client, &text.to_string(), Some(AREA), Instant::now());
    }

    #[test]
    fn answers_with_the_tunnel_as_a_hold_and_counts_no_window() {
        let (mut app, _rx) = app();

        hold(&mut app, HOLD, 5173, 5174);

        let mut answers = app.take_answers();
        let list = reply_of(&answers.remove(0).1).expect("the list");
        assert_eq!(list["tunnels"][0]["kind"], json!("hold"));
        assert_eq!(list["tunnels"][0]["local"], json!(5174));
        assert_eq!(list["windows"], json!(0));
    }

    #[test]
    fn leaves_the_open_request_unavailable_when_only_a_hold_exists() {
        let (mut app, _rx) = app();
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();

        send(&mut app, "tunnel-open", 5173, Instant::now());

        assert_eq!(answer(&mut app).expect_err("refused").code, "unavailable");
    }

    #[test]
    fn an_existing_hold_does_not_hide_a_window_that_could_not_open() {
        let (mut app, _rx) = app();
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);
        app.take_tunnel_orders();

        let down = tunnel(5173, None, TunnelState::Down("refused".into()));
        app.tunnels_reported(WINDOW, holding(&app, vec![down]));
        app.check_requests(now);

        let refused = answer(&mut app).expect_err("failed");
        assert_eq!(refused.message, "could not tunnel port 5173: refused");
    }

    #[test]
    fn closing_its_port_ends_its_stream_and_orders_no_window() {
        let (mut app, _rx) = app();
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();
        let now = Instant::now();

        send(&mut app, "tunnel-close", 5173, now);

        let ended: Vec<u64> = app
            .take_events()
            .into_iter()
            .filter(|(_, streamed)| matches!(streamed, Streamed::End))
            .map(|(c, _)| c)
            .collect();
        assert_eq!(ended, [HOLD]);
        assert_eq!(app.take_tunnel_orders(), Vec::new());
        app.forget(HOLD);
        app.check_requests(now);
        assert_eq!(answer(&mut app).expect("closed")["tunnels"], json!([]));
    }

    #[rstest]
    #[case::closed("tunnel-close", "closed")]
    #[case::forgotten("tunnel-forget", "forgotten")]
    fn a_close_or_a_forget_tells_the_hold_which_before_its_stream_ends(#[case] command: &str, #[case] ended: &str) {
        let (mut app, _rx) = app();
        attached(&mut app);
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Up)]));
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();
        app.take_tunnel_orders();

        send(&mut app, command, 5173, Instant::now());

        let events: Vec<_> = app.take_events().into_iter().filter(|(client, _)| *client == HOLD).collect();
        let [(_, Streamed::Event(line)), (_, Streamed::End)] = events.as_slice() else { panic!("{events:?}") };
        let line: Value = serde_json::from_str(line).expect("json");
        assert_eq!(line, json!({"ok": {"ended": ended}}));
    }

    #[test]
    fn a_window_that_holds_the_port_is_ordered_and_told_no_line() {
        let (mut app, _rx) = app();
        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5173), TunnelState::Up)]));
        app.take_events();

        send(&mut app, "tunnel-close", 5173, Instant::now());

        assert_eq!(app.take_events(), Vec::new());
        assert_eq!(app.take_tunnel_orders().len(), 1);
    }

    #[test]
    fn a_second_hold_on_the_connection_conflicts_and_zero_ports_are_malformed() {
        let (mut app, _rx) = app();
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();

        hold(&mut app, HOLD, 3000, 3000);
        assert_eq!(answer(&mut app).expect_err("refused").code, "conflict");

        hold(&mut app, HOLD + 1, 0, 5);
        assert_eq!(answer(&mut app).expect_err("refused").code, "malformed");
        hold(&mut app, HOLD + 1, 5, 0);
        assert_eq!(answer(&mut app).expect_err("refused").code, "malformed");
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
        assert!(lines[0].get("kind").is_none(), "{lines:?}");
    }

    #[test]
    fn a_closed_tunnel_publishes_its_state_and_a_hold_leaving_says_so() {
        let (mut app, _rx) = app();
        watch_events(&mut app);

        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(3000, None, TunnelState::Closed)]));
        let text = json!({"contract": 1, "command": "tunnel-hold", "args": {"port": 5173, "local": 5174}});
        app.request(HOLD, &text.to_string(), Some(AREA), Instant::now());
        app.forget(HOLD);

        let lines = event_lines(&mut app);
        let seen: Vec<(&Value, Option<&Value>)> = lines.iter().map(|line| (&line["state"], line.get("kind"))).collect();
        let hold = json!("hold");
        assert_eq!(
            seen,
            [(&json!("closed"), None), (&json!("up"), Some(&hold)), (&json!("gone"), Some(&hold))],
            "{lines:?}"
        );
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

mod taking_orders {
    use super::*;

    fn take_orders(app: &mut App, client: u64) {
        let text = json!({"contract": 1, "command": "tunnel-orders", "args": {}});
        app.request(client, &text.to_string(), Some(AREA), Instant::now());
    }

    fn fail(app: &mut App, port: u16, error: &str) {
        let text = json!({"contract": 1, "command": "tunnel-failed", "args": {"port": port, "error": error}});
        app.request(REPORTER, &text.to_string(), Some(AREA), Instant::now());
    }

    fn hold(app: &mut App, client: u64, port: u16, local: u16) {
        let text = json!({"contract": 1, "command": "tunnel-hold", "args": {"port": port, "local": local}});
        app.request(client, &text.to_string(), Some(AREA), Instant::now());
    }

    fn taken(app: &mut App) {
        take_orders(app, TAKER);
        app.take_answers();
    }

    #[test]
    fn is_counted_beside_the_windows_and_answers_with_the_list() {
        let (mut app, _rx) = app();

        take_orders(&mut app, TAKER);

        let mut answers = app.take_answers();
        let list = reply_of(&answers.remove(0).1).expect("the list");
        assert_eq!(list, json!({"tunnels": [], "windows": 0, "takers": 1}));
        let listed = call(&mut app, "tunnels", &json!({})).expect("a list");
        assert_eq!(listed["takers"], json!(1));
    }

    #[test]
    fn an_open_tells_the_program_and_waits_for_its_hold() {
        let (mut app, _rx) = app();
        taken(&mut app);
        let now = Instant::now();

        send(&mut app, "tunnel-open", 5173, now);
        app.check_requests(now);

        assert_eq!(lines_for(&mut app, TAKER), [json!({"order": "open", "port": 5173})]);
        assert_eq!(app.take_tunnel_orders(), Vec::new());
        assert_eq!(app.take_answers(), Vec::new());
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();
        app.check_requests(now);
        let list = answer(&mut app).expect("held");
        assert_eq!(list["tunnels"][0]["kind"], json!("hold"));
    }

    #[test]
    fn a_reported_failure_fails_the_open_with_its_reason() {
        let (mut app, _rx) = app();
        taken(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        fail(&mut app, 5173, "the user said no");
        assert!(answer(&mut app).is_ok());
        app.check_requests(now);

        let refused = answer(&mut app).expect_err("failed");
        assert_eq!(
            (refused.code.as_str(), refused.message.as_str()),
            ("failed", "could not tunnel port 5173: the user said no")
        );
    }

    #[test]
    fn a_failure_reported_before_the_open_does_not_answer_it() {
        let (mut app, _rx) = app();
        taken(&mut app);
        fail(&mut app, 5173, "old news");
        assert!(answer(&mut app).is_ok());
        let now = Instant::now();

        send(&mut app, "tunnel-open", 5173, now);
        app.check_requests(now);

        assert_eq!(app.take_answers(), Vec::new());
    }

    #[test]
    fn with_two_programs_one_failure_leaves_the_open_waiting() {
        let (mut app, _rx) = app();
        taken(&mut app);
        take_orders(&mut app, OTHER_TAKER);
        app.take_answers();
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        fail(&mut app, 5173, "no");
        app.take_answers();
        app.check_requests(now);
        assert_eq!(app.take_answers(), Vec::new());
        hold(&mut app, HOLD, 5173, 5173);
        app.take_answers();
        app.check_requests(now);

        assert_eq!(answer(&mut app).expect("held")["tunnels"][0]["kind"], json!("hold"));
    }

    #[test]
    fn a_window_that_comes_up_answers_without_waiting_for_the_program() {
        let (mut app, _rx) = app();
        taken(&mut app);
        attached(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);
        app.take_tunnel_orders();

        app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(5173, Some(5174), TunnelState::Up)]));
        app.check_requests(now);

        assert_eq!(answer(&mut app).expect("up")["tunnels"][0]["kind"], json!("window"));
    }

    #[test]
    fn the_program_leaving_answers_the_open_and_stops_counting() {
        let (mut app, _rx) = app();
        taken(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.forget(TAKER);
        app.check_requests(now);

        let refused = answer(&mut app).expect_err("failed");
        assert_eq!(refused.message, "could not tunnel port 5173: no program answered");
        assert_eq!(call(&mut app, "tunnels", &json!({})).expect("a list")["takers"], json!(0));
        send(&mut app, "tunnel-open", 5173, now);
        let refused = answer(&mut app).expect_err("refused");
        assert_eq!(refused.code, "unavailable");
        assert!(refused.message.contains("tunnel orders"), "{}", refused.message);
    }

    #[test]
    fn times_out_naming_the_programs() {
        let (mut app, _rx) = app();
        taken(&mut app);
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        app.check_requests(now + Duration::from_secs(46));

        let refused = answer(&mut app).expect_err("timed out");
        let said = "timed out after 45s: the programs that take tunnel orders have not said";
        assert!(refused.message.starts_with(said), "{}", refused.message);
    }

    #[test]
    fn close_and_forget_are_told_to_the_program_even_when_nothing_lists_the_port() {
        let (mut app, _rx) = app();
        taken(&mut app);
        let now = Instant::now();

        send(&mut app, "tunnel-close", 5173, now);
        send(&mut app, "tunnel-forget", 5173, now);
        app.check_requests(now);

        let mut answers = app.take_answers();
        assert_eq!(answers.len(), 2);
        assert!(reply_of(&answers.remove(0).1).is_ok() && reply_of(&answers.remove(0).1).is_ok());
        let lines = lines_for(&mut app, TAKER);
        assert_eq!(lines, [json!({"order": "close", "port": 5173}), json!({"order": "forget", "port": 5173})]);
    }

    #[test]
    fn close_still_ends_a_hold_and_tells_the_program() {
        let (mut app, _rx) = app();
        taken(&mut app);
        hold(&mut app, HOLD, 5173, 5174);
        app.take_answers();

        send(&mut app, "tunnel-close", 5173, Instant::now());

        let events = app.take_events();
        assert!(events.iter().any(|(client, streamed)| *client == HOLD && matches!(streamed, Streamed::End)));
        let told = events.iter().any(|(client, streamed)| {
            *client == TAKER && matches!(streamed, Streamed::Event(text) if text.contains("\"close\""))
        });
        assert!(told, "{events:?}");
    }

    #[test]
    fn a_connection_cannot_both_hold_a_tunnel_and_take_orders() {
        let (mut app, _rx) = app();
        taken(&mut app);

        take_orders(&mut app, TAKER);
        assert_eq!(answer(&mut app).expect_err("twice").code, "conflict");
        hold(&mut app, TAKER, 5173, 5173);
        assert_eq!(answer(&mut app).expect_err("holding").code, "conflict");
        hold(&mut app, HOLD, 5173, 5173);
        app.take_answers();
        take_orders(&mut app, HOLD);
        assert_eq!(answer(&mut app).expect_err("taking").code, "conflict");
    }

    #[test]
    fn a_failure_needs_a_port_and_a_reason_and_is_kept_to_one_line() {
        let (mut app, _rx) = app();
        taken(&mut app);
        fail(&mut app, 0, "no");
        assert_eq!(answer(&mut app).expect_err("no port").code, "malformed");
        fail(&mut app, 5173, "  ");
        assert_eq!(answer(&mut app).expect_err("no reason").code, "malformed");
        let now = Instant::now();
        send(&mut app, "tunnel-open", 5173, now);

        fail(&mut app, 5173, &format!("first\nsecond {}", "x".repeat(400)));
        app.take_answers();
        app.check_requests(now);

        let message = answer(&mut app).expect_err("failed").message;
        let reason = message.strip_prefix("could not tunnel port 5173: ").expect("prefix");
        assert!(reason.starts_with("first second") && !reason.contains('\n') && reason.chars().count() == 300);
    }
}

mod reverse;
