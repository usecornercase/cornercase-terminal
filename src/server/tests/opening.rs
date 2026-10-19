use super::a_bug::{Attached, hello};
use super::*;
use crate::graphics::Support;
use crate::host::open::{Failed, Opener};

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
