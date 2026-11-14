use std::net::TcpStream;
use std::time::Instant;

use tungstenite::protocol::WebSocketConfig;
use tungstenite::protocol::frame::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Message, WebSocket};

use super::Context;
use crate::log;
use crate::web::http::{self, Request, Response};
use crate::web::{Event, WRITE_TIMEOUT};

const NOT_A_SOCKET: &str = "a WebSocket is needed: open the pairing code's address with one";
const BAD_CODE: &str = "this code does not work: show a new one in Settings › Remote";

pub(super) fn pair(mut stream: TcpStream, request: &Request, context: &Context) {
    if !request.wants_websocket() {
        let _ = Response::text("426 Upgrade Required", NOT_A_SOCKET).send(&mut stream);
        return;
    }
    if !request.upgradable() {
        let _ = Response::text("400 Bad Request", "not a WebSocket handshake of version 13").send(&mut stream);
        return;
    }
    let code = request.query("code").unwrap_or_default();
    let accepted = context.shared.lock().pairing.as_ref().is_some_and(|pairing| pairing.accepts(code, Instant::now()));
    if !accepted {
        log::warning!("web", "refused a pairing code");
        let _ = Response::text("403 Forbidden", BAD_CODE).send(&mut stream);
        return;
    }
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let Some(mut ws) = http::upgrade(stream, request, WebSocketConfig::default()) else { return };
    exchange(&mut ws, request, context);
    let _ = ws.flush();
}

fn exchange(ws: &mut WebSocket<TcpStream>, request: &Request, context: &Context) {
    let name = request.header("user-agent").unwrap_or_default();
    let (held, added) = {
        let mut shared = context.shared.lock();
        (shared.pairing.clone(), shared.pair_with(code_of(request), name, Instant::now()))
    };
    match added {
        Some(Ok((id, secret, taken))) => deliver(ws, context, (&id, &secret, taken), (held, name)),
        Some(Err(e)) => {
            log::error!("web", "could not save a paired phone app", error = e);
            end(ws, CloseCode::Error);
        }
        None => {
            log::warning!("web", "refused a pairing code");
            end(ws, CloseCode::Policy);
        }
    }
}

fn code_of(request: &Request) -> &str {
    request.query("code").unwrap_or_default()
}

fn end(ws: &mut WebSocket<TcpStream>, code: CloseCode) {
    let _ = ws.close(Some(CloseFrame { code, reason: "".into() }));
    let _ = ws.flush();
}

fn deliver(
    ws: &mut WebSocket<TcpStream>,
    context: &Context,
    (id, secret, taken): (&str, &str, u64),
    (held, name): (Option<crate::web::auth::Pairing>, &str),
) {
    let reply = serde_json::json!({ "secret": secret }).to_string();
    let delivered = ws.write(Message::text(reply)).and_then(|()| ws.flush()).is_ok();
    let name: String = name.chars().take(120).collect();
    log::info!("web", "phone app paired", device = id, name = name, delivered = delivered);
    if delivered {
        end(ws, CloseCode::Normal);
        context.tell(Event::AppPaired(taken));
        return;
    }
    let mut shared = context.shared.lock();
    let _ = shared.devices.remove(id);
    shared.pairing = held;
    shared.taken = shared.taken.saturating_sub(1);
}
