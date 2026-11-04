use super::a_bug::{Attached, COLS, Client, ROWS, hello};
use super::*;
use crate::graphics::Support;
use crate::protocol::MachineOrder;

const BUTTON: &str = "this machine ▴";

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
    let mut attached = Attached::new();
    let mut switcher = attached.join_hello(knows(&["devbox", "gpu-box"]));
    attached.serve(Server::step, move |plain| {
        switcher.click_on(BUTTON);
        switcher.shows("gpu-box");
        switcher.click_on("gpu-box");
        let got = orders(&switcher.received(ordered));
        assert_eq!(got, [MachineOrder::Switch(Some("gpu-box".into()))]);
        let heard = plain.received(|_| true);
        assert!(orders(&heard).is_empty(), "a window that picked nothing was told to switch");
    });
}

fn from_the_menu(entry: &'static str, then: impl FnOnce(&mut Client) + Send + 'static) {
    let mut attached = Attached::new();
    let mut switcher = attached.join_hello(knows(&["devbox"]));
    attached.serve(Server::step, move |_| {
        switcher.click_on(BUTTON);
        switcher.click_on(entry);
        then(&mut switcher);
    });
}

#[test]
fn a_new_machine_is_typed_in_its_own_dialog() {
    from_the_menu("add machine…", |switcher| {
        switcher.shows("ssh destination");
        "me@new-box".chars().for_each(|c| switcher.key(KeyCode::Char(c)));
        switcher.key(KeyCode::Enter);
        assert_eq!(orders(&switcher.received(ordered)), [MachineOrder::Switch(Some("me@new-box".into()))]);
    });
}

#[test]
fn forgetting_a_machine_tells_its_window() {
    from_the_menu("forget…", |switcher| {
        switcher.click_on("forget devbox");
        assert_eq!(orders(&switcher.received(ordered)), [MachineOrder::Forget("devbox".into())]);
    });
}

#[test]
fn the_button_follows_the_machines_the_window_sends() {
    let mut attached = Attached::new();
    let mut switcher = attached.join_hello(knows(&["devbox"]));
    attached.serve(Server::step, move |_| {
        switcher.shows(BUTTON);
        let none = Machines { here: None, known: Vec::new() };
        switcher.send(ServerEvent::Message(switcher.id, ClientMessage::Machines(none)));
        switcher.until("the button goes", |client| !client.text().contains(BUTTON));
    });
}

#[test]
fn a_window_used_less_recently_does_not_take_the_button() {
    let mut attached = Attached::new();
    let quiet = attached.join_hello(knows(&["devbox"]));
    let on_gpu = Machines { here: Some("gpu-box".into()), known: Vec::new() };
    let mut used = attached.join_hello(Hello { machines: Some(on_gpu), ..hello(HostTheme::default()) });
    attached.serve(Server::step, move |_| {
        used.shows("gpu-box ▴");
        let changed = Machines { here: None, known: vec!["devbox".into(), "other".into()] };
        quiet.send(ServerEvent::Message(quiet.id, ClientMessage::Machines(changed)));
        let before = used.clears;
        used.input(Event::Resize(COLS, ROWS));
        used.until("the screen is drawn again", |client| client.clears > before);
        used.shows("Worktrees");
        assert!(used.text().contains("gpu-box ▴"), "{}", used.text());
    });
}

#[test]
fn a_remote_window_names_the_machine_it_is_on() {
    let mut attached = Attached::new();
    let machines = Machines { here: Some("devbox".into()), known: Vec::new() };
    let mut remote = attached.join_hello(Hello { machines: Some(machines), ..hello(HostTheme::default()) });
    attached.serve(Server::step, move |_| {
        remote.click_on("devbox ▴");
        remote.shows("✓ devbox");
        remote.click_on("this machine");
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
