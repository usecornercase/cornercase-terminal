use serde_json::json;

use super::a_bug::{Attached, COLS, ROWS, hello, one_shell_in};
use super::*;
use crate::graphics::Support;
use crate::protocol::MachineOrder;
use crate::test_util::TempDir;

const BUTTON: &str = "this computer ▴";

fn listing(names: &[&str]) -> Attached {
    let dir = TempDir::new();
    let entries: Vec<_> = names
        .iter()
        .map(|name| json!({"name": name, "icon": "☁", "destination": format!("{name}.lan"), "folder": "~", "shown": false}))
        .collect();
    std::fs::write(dir.path().join("config.json"), json!({"machines": entries}).to_string()).expect("a config");
    let saved = one_shell_in(dir.path());
    Attached::with(dir, &saved)
}

fn knows(known: &[&str]) -> Hello {
    let machines = Machines { here: None, known: known.iter().map(|&name| name.into()).collect() };
    Hello { machines: Some(machines), ..hello(HostTheme::default()) }
}

fn ordered(m: &ServerMessage) -> bool {
    matches!(m, ServerMessage::Machine(_))
}

fn orders(messages: &[ServerMessage]) -> Vec<MachineOrder> {
    messages
        .iter()
        .filter_map(|m| if let ServerMessage::Machine(order) = m { Some(order.clone()) } else { None })
        .collect()
}

#[test]
fn a_window_that_knows_no_other_machine_shows_no_button() {
    Attached::new().serve(Server::step, |client| {
        client.shows("Worktrees");
        assert!(!client.text().contains(BUTTON));
    });
}

#[test]
fn only_the_window_that_picked_a_machine_is_told_to_switch() {
    let mut attached = listing(&["devbox", "gpu-box"]);
    let mut switcher = attached.join_hello(knows(&[]));
    attached.serve(Server::step, move |plain| {
        switcher.click_on(BUTTON);
        switcher.shows("gpu-box");
        switcher.click_on("gpu-box");
        switcher.shows("open the window there");
        switcher.click_on("open the window there");
        let got = orders(&switcher.received(ordered));
        assert_eq!(got, [MachineOrder::Switch(Some("gpu-box".into()))]);
        let heard = plain.received(|_| true);
        assert!(orders(&heard).is_empty(), "a window that picked nothing was told to switch");
    });
}

#[test]
fn add_machine_opens_the_form_of_settings() {
    let mut attached = listing(&["devbox"]);
    let mut switcher = attached.join_hello(knows(&[]));
    attached.serve(Server::step, move |_| {
        switcher.click_on(BUTTON);
        switcher.click_on("add machine…");
        switcher.shows("show its work here");
    });
}

#[test]
fn the_button_shows_from_the_servers_own_list() {
    let attached = listing(&["devbox"]);
    attached.serve(Server::step, |client| client.shows(BUTTON));
}

#[test]
fn a_window_used_less_recently_does_not_take_the_button() {
    let mut attached = Attached::new();
    let quiet = attached.join_hello(knows(&["devbox"]));
    let on_gpu = Machines { here: Some("gpu-box".into()), known: Vec::new() };
    let mut used = attached.join_hello(Hello { machines: Some(on_gpu), ..hello(HostTheme::default()) });
    attached.serve(Server::step, move |_| {
        used.shows("on gpu-box ▴");
        let changed = Machines { here: None, known: vec!["devbox".into(), "other".into()] };
        quiet.send(ServerEvent::Message(quiet.id, ClientMessage::Machines(changed)));
        let before = used.clears;
        used.input(Event::Resize(COLS, ROWS));
        used.until("the screen is drawn again", |client| client.clears > before);
        used.shows("Worktrees");
        assert!(used.text().contains("on gpu-box ▴"), "{}", used.text());
    });
}

#[test]
fn a_remote_window_names_the_machine_it_is_on() {
    let mut attached = Attached::new();
    let machines = Machines { here: Some("devbox".into()), known: Vec::new() };
    let mut remote = attached.join_hello(Hello { machines: Some(machines), ..hello(HostTheme::default()) });
    attached.serve(Server::step, move |_| {
        remote.click_on("on devbox ▴");
        remote.shows("✓ devbox");
        remote.click_on("this computer");
        assert_eq!(orders(&remote.received(ordered)), [MachineOrder::Switch(None)]);
    });
}

#[test]
fn a_browser_is_never_offered_another_machine() {
    let mut attached = Attached::new();
    let mut browser = attached.join_hello(Hello {
        terminal: Some(protocol::BROWSER.into()),
        graphics: Support::default(),
        ..knows(&["devbox"])
    });
    attached.serve(Server::step, move |_| {
        browser.shows("Worktrees");
        assert!(!browser.text().contains(BUTTON));
    });
}
