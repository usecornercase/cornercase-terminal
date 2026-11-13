use super::a_bug::{Attached, Client, LISTEN, hello};
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

fn request(client: &Client, text: &str) {
    client.send(ServerEvent::Message(client.id, ClientMessage::Request(text.into())));
}

#[test]
fn a_connection_that_holds_a_tunnel_is_detached_and_removed_when_the_tunnel_is_closed() {
    let mut attached = Attached::new();
    let holder = attached.join_hello(a_window(None));
    let closer = attached.join_hello(a_window(None));
    attached.serve(Server::step, move |_| {
        request(&holder, r#"{"contract":1,"command":"tunnel-hold","args":{"port":5173,"local":5174}}"#);
        let answers = holder.received(|m| matches!(m, ServerMessage::Response(_)));
        assert!(answers.iter().any(|m| matches!(m, ServerMessage::Response(text) if text.contains("hold"))));

        request(&closer, r#"{"contract":1,"command":"tunnel-close","args":{"port":5173}}"#);

        holder.received(|m| matches!(m, ServerMessage::Detached));
        let said = closer.received(|m| matches!(m, ServerMessage::Response(_)));
        assert!(said.iter().any(|m| matches!(m, ServerMessage::Response(text) if text.contains("\"tunnels\":[]"))));
    });
}

#[test]
fn a_connection_taking_orders_hears_an_open_as_a_line_and_its_leaving_answers_it() {
    let mut attached = Attached::new();
    let taker = attached.join_hello(a_window(None));
    let asker = attached.join_hello(a_window(None));
    attached.serve(Server::step, move |_| {
        request(&taker, r#"{"contract":1,"command":"tunnel-orders","args":{}}"#);
        let taking = taker.received(|m| matches!(m, ServerMessage::Response(_)));
        assert!(taking.iter().any(|m| matches!(m, ServerMessage::Response(text) if text.contains("\"takers\":1"))));

        request(&asker, r#"{"contract":1,"command":"tunnel-open","args":{"port":5173}}"#);

        let heard =
            taker.received(|m| matches!(m, ServerMessage::Response(text) if text.contains("\"order\":\"open\"")));
        assert!(orders(&heard).is_empty(), "a program with no window was sent a postcard order");
        taker.send(ServerEvent::Gone(taker.id));
        let said = asker.received(|m| matches!(m, ServerMessage::Response(_)));
        assert!(
            said.iter().any(|m| matches!(m, ServerMessage::Response(text) if text.contains("no program answered")))
        );
    });
}
