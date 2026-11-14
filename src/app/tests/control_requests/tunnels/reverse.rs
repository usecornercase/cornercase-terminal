use super::*;

const REVERSE: u64 = HOLD;
const FORWARD: u64 = HOLD + 1;

fn hold_in(app: &mut App, client: u64, port: u16, local: u16, direction: Option<&str>) {
    let mut args = json!({"port": port, "local": local});
    if let Some(direction) = direction {
        args["direction"] = json!(direction);
    }
    let text = json!({"contract": 1, "command": "tunnel-hold", "args": args});
    app.request(client, &text.to_string(), Some(AREA), Instant::now());
}

fn hold_reverse(app: &mut App, client: u64, port: u16, local: u16) {
    hold_in(app, client, port, local, Some("reverse"));
}

fn hold_forward(app: &mut App, client: u64, port: u16, local: u16) {
    hold_in(app, client, port, local, None);
}

fn end(app: &mut App, command: &str, port: u16, direction: Option<&str>, now: Instant) {
    let mut args = json!({"port": port});
    if let Some(direction) = direction {
        args["direction"] = json!(direction);
    }
    let text = json!({"contract": 1, "command": command, "args": args});
    app.request(CLIENT, &text.to_string(), Some(AREA), now);
}

fn ended(app: &mut App) -> Vec<u64> {
    app.take_events()
        .into_iter()
        .filter(|(_, streamed)| matches!(streamed, Streamed::End))
        .map(|(client, _)| client)
        .collect()
}

fn take_orders(app: &mut App, client: u64) {
    let text = json!({"contract": 1, "command": "tunnel-orders", "args": {}});
    app.request(client, &text.to_string(), Some(AREA), Instant::now());
    app.take_answers();
}

#[test]
fn a_reverse_hold_is_listed_with_its_direction_and_a_forward_one_without() {
    let (mut app, _rx) = app();
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    hold_forward(&mut app, FORWARD, 5173, 5174);
    app.take_answers();

    let list = call(&mut app, "tunnels", &json!({})).expect("a list");

    assert_eq!(list["tunnels"][1]["port"], json!(9223));
    assert_eq!(list["tunnels"][1]["kind"], json!("hold"));
    assert_eq!(list["tunnels"][1]["direction"], json!("reverse"));
    assert_eq!(list["tunnels"][0]["port"], json!(5173));
    assert!(list["tunnels"][0].get("direction").is_none(), "{list}");
    assert_eq!(list["windows"], json!(0));
}

#[test]
fn closing_a_port_leaves_the_reverse_hold_of_that_port_alone() {
    let (mut app, _rx) = app();
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    hold_forward(&mut app, FORWARD, 9223, 5174);
    app.take_answers();
    let now = Instant::now();

    end(&mut app, "tunnel-close", 9223, None, now);

    assert_eq!(ended(&mut app), [FORWARD]);
    app.forget(FORWARD);
    app.check_requests(now);
    let list = answer(&mut app).expect("closed");
    assert_eq!(list["tunnels"][0]["direction"], json!("reverse"));
    end(&mut app, "tunnel-close", 9223, Some("reverse"), now);
    assert_eq!(ended(&mut app), [REVERSE]);
    app.forget(REVERSE);
    app.check_requests(now);
    assert_eq!(answer(&mut app).expect("closed")["tunnels"], json!([]));
}

#[test]
fn forgetting_a_reverse_port_leaves_the_forward_tunnel_alone() {
    let (mut app, _rx) = app();
    app.tunnels_reported(WINDOW, holding(&app, vec![tunnel(9223, Some(9223), TunnelState::Up)]));
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.take_answers();
    let now = Instant::now();

    end(&mut app, "tunnel-forget", 9223, Some("reverse"), now);

    assert_eq!(ended(&mut app), [REVERSE]);
    assert_eq!(app.take_tunnel_orders(), Vec::new());
    app.forget(REVERSE);
    app.check_requests(now);
    let list = answer(&mut app).expect("forgotten");
    assert_eq!(list["tunnels"][0]["holder"], json!(WINDOW));
    assert_eq!(list["tunnels"].as_array().map(Vec::len), Some(1));
}

#[test]
fn a_reverse_close_with_nothing_held_is_not_found_even_while_a_program_takes_orders() {
    let (mut app, _rx) = app();
    take_orders(&mut app, TAKER);

    end(&mut app, "tunnel-close", 9223, Some("reverse"), Instant::now());

    let refused = answer(&mut app).expect_err("refused");
    assert_eq!(refused.code, "not-found");
    assert!(refused.message.contains("reverse tunnel"), "{}", refused.message);
    assert_eq!(lines_for(&mut app, TAKER), Vec::<Value>::new());
}

#[test]
fn a_reverse_close_is_not_told_to_programs_that_take_orders() {
    let (mut app, _rx) = app();
    take_orders(&mut app, TAKER);
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.take_answers();

    end(&mut app, "tunnel-close", 9223, Some("reverse"), Instant::now());
    end(&mut app, "tunnel-forget", 9223, Some("reverse"), Instant::now());

    assert!(ended(&mut app).contains(&REVERSE));
    assert_eq!(lines_for(&mut app, TAKER), Vec::<Value>::new());
}

#[test]
fn a_reverse_hold_never_settles_an_open() {
    let (mut app, _rx) = app();
    take_orders(&mut app, TAKER);
    let now = Instant::now();
    send(&mut app, "tunnel-open", 9223, now);
    app.take_answers();

    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.take_answers();
    app.check_requests(now);
    assert_eq!(app.take_answers(), Vec::new());

    hold_forward(&mut app, FORWARD, 9223, 5174);
    app.take_answers();
    app.check_requests(now);
    let list = answer(&mut app).expect("held");
    assert_eq!(list["tunnels"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["tunnels"][0]["holder"], json!(FORWARD));
    assert!(list["tunnels"][0].get("direction").is_none(), "{list}");
}

#[test]
fn opening_a_reverse_tunnel_is_malformed() {
    let (mut app, _rx) = app();
    attached(&mut app);

    for direction in ["reverse", "sideways"] {
        let text = json!({"contract": 1, "command": "tunnel-open", "args": {"port": 9223, "direction": direction}});
        app.request(CLIENT, &text.to_string(), Some(AREA), Instant::now());

        assert_eq!(answer(&mut app).expect_err("refused").code, "malformed", "{direction}");
    }
    assert_eq!(app.take_tunnel_orders(), Vec::new());
}

#[test]
fn a_reverse_hold_s_events_carry_its_direction() {
    let (mut app, _rx) = app();
    watch_events(&mut app);

    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.forget(REVERSE);

    let lines = event_lines(&mut app);
    let seen: Vec<(&Value, &Value, &Value)> =
        lines.iter().map(|line| (&line["state"], &line["kind"], &line["direction"])).collect();
    let (hold, reverse) = (json!("hold"), json!("reverse"));
    assert_eq!(seen, [(&json!("up"), &hold, &reverse), (&json!("gone"), &hold, &reverse)], "{lines:?}");
    assert_eq!((lines[0]["port"].clone(), lines[0]["local"].clone()), (json!(9223), json!(9222)));
}

#[test]
fn status_lists_a_reverse_hold_with_its_direction() {
    let (mut app, _rx) = app();
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.take_answers();

    let report = call(&mut app, "status", &json!({})).expect("a report");

    assert_eq!(report["tunnels"][0]["direction"], json!("reverse"));
}

#[test]
fn a_connection_that_holds_a_reverse_tunnel_cannot_take_orders_or_hold_again() {
    let (mut app, _rx) = app();
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    app.take_answers();

    hold_reverse(&mut app, REVERSE, 9224, 9222);
    assert_eq!(answer(&mut app).expect_err("holding").code, "conflict");
    let text = json!({"contract": 1, "command": "tunnel-orders", "args": {}});
    app.request(REVERSE, &text.to_string(), Some(AREA), Instant::now());
    assert_eq!(answer(&mut app).expect_err("taking").code, "conflict");
    take_orders(&mut app, TAKER);
    hold_reverse(&mut app, TAKER, 9225, 9222);
    assert_eq!(answer(&mut app).expect_err("taking").code, "conflict");
}

#[rstest::rstest]
#[case::closed("tunnel-close", "closed")]
#[case::forgotten("tunnel-forget", "forgotten")]
fn a_reverse_end_tells_the_reverse_hold_which_and_no_other(#[case] command: &str, #[case] ended: &str) {
    let (mut app, _rx) = app();
    hold_reverse(&mut app, REVERSE, 9223, 9222);
    hold_forward(&mut app, FORWARD, 9223, 5174);
    app.take_answers();

    end(&mut app, command, 9223, Some("reverse"), Instant::now());

    let events = app.take_events();
    let [(REVERSE, Streamed::Event(line)), (REVERSE, Streamed::End)] = events.as_slice() else { panic!("{events:?}") };
    let line: Value = serde_json::from_str(line).expect("json");
    assert_eq!(line, json!({"ok": {"ended": ended}}));
}
