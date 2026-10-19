use std::io::{self, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::Event as Input;
use libghostty_vt::style::RgbColor;
use serde::Deserialize;
use tungstenite::protocol::{Role, WebSocketConfig};
use tungstenite::{Message, WebSocket};

use super::Context;
use super::http::Request;
use crate::client::input::Parser;
use crate::graphics::{Missing, Support};
use crate::host::notify::Channel;
use crate::host::theme::{HostTheme, PALETTE_LEN};
use crate::log;
use crate::protocol::{self, ClientMessage, Hello, ServerMessage};
use crate::version;

const POLL: Duration = Duration::from_millis(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const HELLO_WITHIN: Duration = Duration::from_secs(10);
const PING_EVERY: Duration = Duration::from_secs(15);
const SILENT_FOR: Duration = Duration::from_secs(45);
const MAX_MESSAGE: usize = 1024 * 1024;
const MODES: &[u8] = b"\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?2004h";
const THEMED: usize = 16;
const QUEUED_FRAMES: usize = 64;
const MAX_COLS: u16 = 1000;
const MAX_ROWS: u16 = 500;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum FromPage {
    Hello {
        cols: u16,
        rows: u16,
        #[serde(default)]
        theme: Theme,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Ping,
}

#[derive(Debug, Default, Deserialize)]
struct Theme {
    #[serde(default)]
    zone: String,
    foreground: Option<String>,
    background: Option<String>,
    #[serde(default)]
    palette: Vec<String>,
}

fn colour(hex: &str) -> Option<RgbColor> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some(RgbColor { r: byte(0)?, g: byte(2)?, b: byte(4)? })
}

fn host_theme(theme: &Theme) -> HostTheme {
    let mut palette = [None; PALETTE_LEN];
    for (slot, hex) in palette.iter_mut().zip(theme.palette.iter().take(THEMED)) {
        *slot = colour(hex);
    }
    HostTheme {
        foreground: theme.foreground.as_deref().and_then(colour),
        background: theme.background.as_deref().and_then(colour),
        palette,
        truecolor: true,
    }
}

fn size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(1, MAX_COLS), rows.clamp(1, MAX_ROWS))
}

fn hello(cols: u16, rows: u16, theme: &Theme) -> Hello {
    let (cols, rows) = size(cols, rows);
    Hello {
        version: protocol::VERSION,
        build: version::build_id(),
        cols,
        rows,
        theme: host_theme(theme),
        notify: Channel::Bell,
        terminal: Some(protocol::BROWSER.into()),
        pointer: None,
        graphics: Support { missing: Some(Missing::Cannot { name: "this browser".into() }), ..Support::default() },
        probe: "web".into(),
        zone: if theme.zone.is_empty() { "UTC".into() } else { theme.zone.clone() },
        openers: Vec::new(),
    }
}

fn end(reason: &str, message: &str) -> Message {
    Message::text(serde_json::json!({ "type": "end", "reason": reason, "message": message }).to_string())
}

fn ping() -> Message {
    Message::text(r#"{"type":"ping"}"#)
}

fn waiting(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

pub(super) fn run(stream: TcpStream, request: &Request, context: &Context) {
    let Some(key) = request.header("sec-websocket-key") else { return };
    if request.header("sec-websocket-version") != Some("13") {
        return;
    }
    let Some(id) = context.register(&stream) else { return };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let mut raw = stream;
    let upgraded = raw.write_all(
        format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Accept: {accept}\r\n\r\n"
        )
        .as_bytes(),
    );
    if upgraded.is_ok() {
        let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE));
        let mut ws = WebSocket::from_raw_socket(raw, Role::Server, Some(config));
        if let Some((cols, rows, theme)) = wait_for_hello(&mut ws) {
            attach(&mut ws, context, hello(cols, rows, &theme));
        }
        let _ = ws.close(None);
        let _ = ws.flush();
    }
    context.unregister(id);
}

fn wait_for_hello(ws: &mut WebSocket<TcpStream>) -> Option<(u16, u16, Theme)> {
    let _ = ws.get_ref().set_read_timeout(Some(HELLO_WITHIN));
    loop {
        match ws.read().ok()? {
            Message::Text(text) => match serde_json::from_str::<FromPage>(&text) {
                Ok(FromPage::Hello { cols, rows, theme }) => return Some((cols, rows, theme)),
                Ok(_) => {}
                Err(_) => return None,
            },
            Message::Close(_) => return None,
            _ => {}
        }
    }
}

fn attach(ws: &mut WebSocket<TcpStream>, context: &Context, hello: Hello) {
    let Ok(mut socket) = UnixStream::connect(&context.socket) else {
        let _ = ws.send(end("gone", "the cornercase server is not running"));
        return;
    };
    let Ok(reader) = socket.try_clone() else { return };
    if protocol::send(&mut socket, &ClientMessage::Hello(Box::new(hello))).is_err() {
        return;
    }
    let from_server = forward(reader);
    let _ = ws.get_ref().set_read_timeout(Some(POLL));
    let _ = ws.get_ref().set_write_timeout(Some(WRITE_TIMEOUT));
    if ws.send(Message::binary(MODES.to_vec())).is_err() {
        return;
    }
    context.browser(1);
    log::info!("web", "browser connected");
    let ending = pump(ws, &mut socket, &from_server);
    let _ = socket.shutdown(std::net::Shutdown::Both);
    context.browser(-1);
    log::info!("web", "browser gone", reason = ending);
}

fn forward(mut reader: UnixStream) -> Receiver<Option<ServerMessage>> {
    let (tx, rx) = mpsc::sync_channel(QUEUED_FRAMES);
    thread::spawn(move || {
        while let Ok(Some(message)) = protocol::recv::<ServerMessage>(&mut reader) {
            if tx.send(Some(message)).is_err() {
                return;
            }
        }
        let _ = tx.send(None);
    });
    rx
}

fn pump(
    ws: &mut WebSocket<TcpStream>,
    socket: &mut UnixStream,
    from_server: &Receiver<Option<ServerMessage>>,
) -> &'static str {
    let mut parser = Parser::default();
    let (mut heard, mut pinged) = (Instant::now(), Instant::now());
    loop {
        match from_page(ws, socket, &mut parser) {
            Ok(true) => heard = Instant::now(),
            Ok(false) => {}
            Err(reason) => return reason,
        }
        if let Some(reason) = to_page(ws, from_server) {
            return reason;
        }
        if pinged.elapsed() >= PING_EVERY {
            pinged = Instant::now();
            if ws.write(ping()).is_err() {
                return "connection lost";
            }
        }
        if ws.flush().is_err() {
            return "connection lost";
        }
        if heard.elapsed() >= SILENT_FOR {
            return "silent";
        }
    }
}

fn from_page(
    ws: &mut WebSocket<TcpStream>,
    socket: &mut UnixStream,
    parser: &mut Parser,
) -> Result<bool, &'static str> {
    match ws.read() {
        Ok(Message::Binary(bytes)) => {
            let mut sent = true;
            parser.feed(&bytes, false, &mut |input| {
                sent = protocol::send(socket, &ClientMessage::from(input)).is_ok();
                sent
            });
            if sent { Ok(true) } else { Err("server gone") }
        }
        Ok(Message::Text(text)) => {
            if let Ok(FromPage::Resize { cols, rows }) = serde_json::from_str(&text) {
                let (cols, rows) = size(cols, rows);
                let resize = Input::Resize(cols, rows);
                protocol::send(socket, &ClientMessage::Event(resize)).map_err(|_| "server gone")?;
            }
            Ok(true)
        }
        Ok(Message::Close(_)) => Err("closed by the page"),
        Ok(_) => Ok(true),
        Err(e) if waiting(&e) => {
            let mut sent = true;
            parser.idle(&mut |input| {
                sent = protocol::send(socket, &ClientMessage::from(input)).is_ok();
                sent
            });
            if sent { Ok(false) } else { Err("server gone") }
        }
        Err(_) => Err("connection lost"),
    }
}

fn to_page(ws: &mut WebSocket<TcpStream>, from_server: &Receiver<Option<ServerMessage>>) -> Option<&'static str> {
    loop {
        match from_server.try_recv() {
            Ok(Some(ServerMessage::Frame(bytes))) => {
                if ws.write(Message::binary(bytes)).is_err() {
                    return Some("connection lost");
                }
            }
            Ok(Some(ServerMessage::Response(_) | ServerMessage::Sound(_) | ServerMessage::Open(_))) => {}
            Ok(Some(ServerMessage::Rejected(why))) => return Some(finish(ws, "rejected", &why)),
            Ok(Some(ServerMessage::Detached)) => return Some(finish(ws, "detached", "")),
            Ok(Some(ServerMessage::Shutdown)) => return Some(finish(ws, "shutdown", "")),
            Ok(Some(ServerMessage::Restart(_))) => return Some(finish(ws, "restart", "")),
            Ok(None) | Err(TryRecvError::Disconnected) => return Some(finish(ws, "gone", "")),
            Err(TryRecvError::Empty) => return None,
        }
    }
}

fn finish(ws: &mut WebSocket<TcpStream>, reason: &'static str, message: &str) -> &'static str {
    let _ = ws.send(end(reason, message));
    reason
}

#[cfg(test)]
mod tests;
