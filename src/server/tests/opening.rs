use super::a_bug::{Attached, LISTEN, hello};
use super::*;
use crate::graphics::Support;
use crate::host::open::{Failed, Link, Opener};

fn can_open() -> Hello {
    Hello { openers: vec![Opener::Files, Opener::Vscode], ..hello(HostTheme::default()) }
}

fn opens(m: &ServerMessage) -> bool {
    matches!(m, ServerMessage::Open(_))
}

#[test]
fn only_the_window_that_picked_an_opener_is_told_to_open() {
    let mut attached = Attached::new();
    let dir = attached.dir().to_path_buf();
    let mut opener = attached.join_hello(can_open());
    attached.serve(Server::step, move |plain| {
        opener.right_click_in_the_pane();
        opener.shows("open in…");
        opener.click_on("open in…");
        opener.shows("VS Code");
        opener.click_on("VS Code");
        let got = opener.received(opens);
        let open = got.iter().find_map(|m| if let ServerMessage::Open(open) = m { Some(open.clone()) } else { None });
        let open = open.expect("an open");
        assert_eq!((open.opener, open.path.as_str(), open.line), (Opener::Vscode, dir.to_str().expect("UTF-8"), None));
        let heard = plain.received(|_| true);
        assert!(!heard.iter().any(opens), "a window that picked nothing was told to open");
    });
}

#[test]
fn a_window_that_can_open_nothing_is_offered_copy_path() {
    Attached::new().serve(Server::step, |client| {
        client.right_click_in_the_pane();
        client.shows("copy path");
        assert!(!client.text().contains("open in…"));
    });
}

#[test]
fn a_browser_that_claims_openers_still_gets_copy_path() {
    let mut attached = Attached::new();
    let mut browser = attached.join_hello(Hello {
        terminal: Some(protocol::BROWSER.into()),
        graphics: Support::default(),
        ..can_open()
    });
    attached.serve(Server::step, move |_| {
        browser.right_click_in_the_pane();
        browser.shows("copy path");
    });
}

#[test]
fn a_failed_open_becomes_a_toast() {
    let mut attached = Attached::new();
    let mut opener = attached.join_hello(can_open());
    attached.serve(Server::step, move |_| {
        let failed = Failed { opener: Opener::Vscode, reason: "exit status 1".into() };
        opener.send(ServerEvent::Message(opener.id, ClientMessage::OpenFailed(failed)));
        opener.shows("could not open in VS Code: exit status 1");
    });
}

#[test]
fn only_a_window_that_can_open_is_told_to_open_a_port_in_the_browser() {
    let mut attached = Attached::new();
    let mut opener = attached.join_hello(Hello { openers: vec![Opener::Default], ..hello(HostTheme::default()) });
    attached.serve(Server::step, move |plain| {
        opener.type_line(LISTEN);
        opener.right_click_on("default");
        opener.click_on("ports");
        opener.shows("http://localhost:");
        opener.click_on(" open ");
        let got = opener.received(|m| matches!(m, ServerMessage::Browse(_)));
        assert!(got.iter().any(|m| matches!(m, ServerMessage::Browse(browse) if browse.ip.is_none())));
        plain.shows("http://localhost:");
        plain.click_on(" open ");
        plain.shows("copied http://localhost:");
        assert!(!plain.received(|_| true).iter().any(|m| matches!(m, ServerMessage::Browse(_))));
    });
}

#[test]
fn a_link_goes_only_to_its_window_when_that_window_can_browse_and_is_no_browser() {
    let mut attached = Attached::new();
    let local = || Hello { openers: vec![Opener::Default], ..hello(HostTheme::default()) };
    let opener = attached.join_hello(local());
    let browser = attached.join_hello(Hello { terminal: Some(protocol::BROWSER.into()), ..local() });
    let plain = attached.join_hello(hello(HostTheme::default()));
    let link = Link { url: "https://example.com/docs".into() };
    for id in [opener.id, browser.id, plain.id] {
        attached.server.app.set_window(id, &[Opener::Default]);
        attached.server.app.send_link(link.clone());
    }
    attached.server.flush_links();
    for client in &attached.server.clients {
        client.send(ServerMessage::Response("done".into()));
    }
    let done = |m: &ServerMessage| matches!(m, ServerMessage::Response(_));
    let links = |got: Vec<ServerMessage>| got.into_iter().filter(|m| matches!(m, ServerMessage::Link(_))).count();
    assert_eq!(links(opener.received(done)), 1);
    assert_eq!(links(browser.received(done)), 0);
    assert_eq!(links(plain.received(done)), 0);
}
