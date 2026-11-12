use super::a_bug::{Attached, LISTEN, hello};
use super::*;
use crate::graphics::Support;
use crate::protocol::{Holding, TunnelOrder};

fn a_window(terminal: Option<&str>) -> Hello {
    Hello {
        machines: Some(Machines::default()),
        terminal: terminal.map(str::to_string),
        graphics: if terminal.is_some() { Support::default() } else { hello(HostTheme::default()).graphics },
        ..hello(HostTheme::default())
    }
}

fn orders(messages: &[ServerMessage]) -> Vec<TunnelOrder> {
    messages.iter().filter_map(|m| if let ServerMessage::Tunnel(order) = m { Some(*order) } else { None }).collect()
}

fn ordered(m: &ServerMessage) -> bool {
    matches!(m, ServerMessage::Tunnel(_))
}

#[test]
fn only_the_window_that_clicked_tunnel_is_told_to_open_it() {
    let mut attached = Attached::new();
    let mut window = attached.join_hello(a_window(None));
    attached.serve(Server::step, move |plain| {
        window.type_line(LISTEN);
        window.send(ServerEvent::Message(window.id, ClientMessage::Tunnels(Holding::default())));
        window.right_click_on("default");
        window.click_on("ports");
        window.shows("http://localhost:");
        window.click_on(" tunnel ");
        let got = orders(&window.received(ordered));
        assert!(matches!(got.as_slice(), [TunnelOrder::Open(port)] if *port > 0), "{got:?}");
        let heard = plain.received(|_| true);
        assert!(orders(&heard).is_empty(), "a window that clicked nothing was told to open a tunnel");
    });
}

#[test]
fn a_browser_cannot_hold_tunnels() {
    let mut attached = Attached::new();
    let mut browser = attached.join_hello(a_window(Some(protocol::BROWSER)));
    attached.serve(Server::step, move |_| {
        browser.type_line(LISTEN);
        browser.send(ServerEvent::Message(browser.id, ClientMessage::Tunnels(Holding::default())));
        browser.right_click_on("default");
        browser.click_on("ports");
        browser.shows("http://localhost:");
        assert!(!browser.text().contains(" tunnel "));
    });
}
