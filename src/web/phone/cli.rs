use std::collections::VecDeque;
use std::io::{ErrorKind, Read as _};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::protocol::frame::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Bytes, Error, Message, WebSocket};

use super::command::{self, Out, Running, Writing};
use super::link::Link;
use super::message::{self, Input, Output};
use super::{Context, Limits, Refused};
use crate::cli;
use crate::log;
use crate::web::http::{self, Request, Response, Timed as _, waiting};
use crate::web::{POLL, WRITE_TIMEOUT};

const STOP_WRITE: Duration = Duration::from_secs(1);
const WRITES_PER_TURN: usize = 4096;
const BUFFERED: usize = 128 * 1024;
const BACKLOG: usize = 4 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum Ending {
    Exited(i32),
    Closed,
    Lost,
    Silent,
    Refused(String),
    Stopped,
}

impl Ending {
    fn why(&self) -> &'static str {
        match self {
            Self::Exited(_) => "exited",
            Self::Closed => "closed",
            Self::Lost => "lost",
            Self::Silent => "silent",
            Self::Refused(_) => "refused",
            Self::Stopped => "stopped",
        }
    }

    fn code(&self) -> Option<i32> {
        match self {
            Self::Exited(code) => Some(*code),
            Self::Refused(_) => Some(2),
            _ => None,
        }
    }
}

enum First {
    Argv(Vec<String>),
    Refused(String),
    Stopped,
    Gone,
}

fn reply(stream: &mut Link, status: &'static str, body: &'static str) {
    let _ = Response::text(status, body).send(stream);
}

fn bearer(request: &Request) -> Option<&str> {
    let (scheme, credential) = request.header("authorization")?.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| credential.trim())
}

fn config(limits: &Limits) -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(limits.largest_message))
        .max_frame_size(Some(limits.largest_message))
        .max_write_buffer_size(BUFFERED + 7 * limits.longest_line)
}

pub(super) fn run(mut stream: Link, request: &Request, context: &Context) {
    if !request.wants_websocket() || !request.upgradable() {
        return refuse_handshake(&mut stream, request, context);
    }
    let (registered, device) = match context.admit(bearer(request), &stream) {
        Ok(admitted) => admitted,
        Err(refused) => return turn_away(&mut stream, &refused),
    };
    let leaving =
        http::upgrade(stream, request, config(&context.limits)).and_then(|ws| connected(ws, context, &device));
    context.unregister(registered);
    if let Some(ws) = leaving {
        drain(ws.get_ref().tcp(), context.limits.linger);
    }
}

fn refuse_handshake(stream: &mut Link, request: &Request, context: &Context) {
    match bearer(request) {
        None => return turn_away(stream, &Refused::NoSecret),
        Some(credential) if context.shared.lock().devices.check(credential).is_none() => {
            return turn_away(stream, &Refused::UnknownSecret);
        }
        Some(_) => {}
    }
    if request.wants_websocket() {
        reply(stream, "400 Bad Request", "not a WebSocket handshake of version 13");
    } else {
        reply(stream, "426 Upgrade Required", "a WebSocket is needed");
    }
}

fn turn_away(stream: &mut Link, refused: &Refused) {
    match refused {
        Refused::Closed => reply(stream, "503 Service Unavailable", "not listening"),
        Refused::NoSecret | Refused::UnknownSecret => {
            let reason = if matches!(refused, Refused::NoSecret) { "no-secret" } else { "unknown-secret" };
            log::warning!("web", "refused a /cli connection", reason = reason);
            reply(stream, "403 Forbidden", "not paired");
        }
    }
}

fn connected(mut ws: WebSocket<Link>, context: &Context, device: &str) -> Option<WebSocket<Link>> {
    let _ = ws.get_ref().wait_at_most(POLL);
    let _ = ws.get_ref().tcp().set_write_timeout(Some(WRITE_TIMEOUT));
    let argv = match first(&mut ws, &context.limits, &context.stopping) {
        First::Argv(argv) => argv,
        First::Refused(text) => {
            refuse(&mut ws, &text, context.limits.linger);
            return None;
        }
        First::Stopped => {
            stopped(&mut ws);
            return Some(ws);
        }
        First::Gone => return None,
    };
    if let Err(text) = cli::over_cli(&argv) {
        refuse(&mut ws, &text, context.limits.linger);
        return None;
    }
    let command = command::command(&context.program, &argv, &context.socket);
    match Running::start(command, context.limits, &context.waits) {
        Ok(running) => converse(ws, running, context, (device, &cli::command_name(&argv))),
        Err(e) => {
            refuse(&mut ws, &format!("could not run cornercase: {e}"), context.limits.linger);
            None
        }
    }
}

fn first(ws: &mut WebSocket<Link>, limits: &Limits, stopping: &AtomicBool) -> First {
    let deadline = Instant::now() + limits.argv_within;
    loop {
        if stopping.load(Ordering::SeqCst) {
            return First::Stopped;
        }
        match ws.read() {
            Ok(Message::Text(text)) => {
                return message::first(&text).map_or_else(First::Refused, First::Argv);
            }
            Ok(Message::Binary(_)) => return First::Refused("the first message must be text, not binary".into()),
            Ok(Message::Close(_)) => return First::Gone,
            Ok(_) => {}
            Err(Error::Capacity(_)) => return First::Refused("a message passed 1 MiB".into()),
            Err(e) if waiting(&e) => {}
            Err(_) => return First::Gone,
        }
        if Instant::now() >= deadline {
            return First::Refused("no `argv` within 10 s".into());
        }
    }
}

fn tell(ws: &mut WebSocket<Link>, output: &Output) -> bool {
    ws.send(Message::text(output.json())).is_ok()
}

fn finish(ws: &mut WebSocket<Link>, code: CloseCode) {
    let _ = ws.close(Some(CloseFrame { code, reason: "".into() }));
    let _ = ws.flush();
}

fn linger(ws: &mut WebSocket<Link>, within: Duration) {
    let end = Instant::now() + within;
    while Instant::now() < end {
        match ws.read() {
            Err(e) if !waiting(&e) => return,
            _ => {}
        }
    }
}

fn stopped(ws: &mut WebSocket<Link>) {
    let _ = ws.get_ref().tcp().set_write_timeout(Some(STOP_WRITE));
    finish(ws, CloseCode::Away);
    ws.get_mut().end();
}

fn drain(mut stream: &TcpStream, within: Duration) {
    let end = Instant::now() + within;
    let mut buffer = [0_u8; 4096];
    while Instant::now() < end {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted) => {}
            Err(_) => return,
        }
    }
}

fn refuse(ws: &mut WebSocket<Link>, text: &str, within: Duration) {
    if tell(ws, &Output::Stderr(message::refusal(text))) && tell(ws, &Output::Exit(2)) {
        finish(ws, CloseCode::Normal);
        linger(ws, within);
    }
    ws.get_mut().end();
}

fn converse(
    ws: WebSocket<Link>,
    running: Running,
    context: &Context,
    (device, name): (&str, &str),
) -> Option<WebSocket<Link>> {
    let (pid, tls) = (running.pid(), ws.get_ref().encrypted());
    log::info!("web", "phone app command started", command = name, device = device, pid = pid, tls = tls);
    let _ = ws.get_ref().tcp().set_write_timeout(Some(POLL));
    let now = Instant::now();
    let mut pump =
        Pump { ws, running, context, heard: now, pinged: now, flushing: false, held: VecDeque::new(), held_bytes: 0 };
    let ending = pump.drive();
    pump.conclude(&ending);
    let code = ending.code().map_or_else(|| "none".to_string(), |code| code.to_string());
    log::info!("web", "phone app command ended", command = name, code = code, why = ending.why());
    (ending == Ending::Stopped).then_some(pump.ws)
}

struct Pump<'a> {
    ws: WebSocket<Link>,
    running: Running,
    context: &'a Context,
    heard: Instant,
    pinged: Instant,
    flushing: bool,
    held: VecDeque<Input>,
    held_bytes: usize,
}

impl Pump<'_> {
    fn drive(&mut self) -> Ending {
        loop {
            if let Some(ending) = self.turn() {
                return ending;
            }
        }
    }

    fn turn(&mut self) -> Option<Ending> {
        if self.context.stopping.load(Ordering::SeqCst) {
            return Some(Ending::Stopped);
        }
        if let Some(ending) = self.hear() {
            return Some(ending);
        }
        if let Some(ending) = self.speak() {
            return Some(ending);
        }
        self.keep_alive().or_else(|| self.running.finished().map(Ending::Exited))
    }

    fn hear(&mut self) -> Option<Ending> {
        self.release_held();
        if self.held_bytes >= BACKLOG {
            thread::sleep(POLL);
            return None;
        }
        match self.ws.read() {
            Ok(Message::Text(text)) => {
                self.heard = Instant::now();
                self.deliver(message::next(&text))
            }
            Ok(Message::Binary(_)) => Some(Ending::Refused("a message must be text, not binary".into())),
            Ok(Message::Close(_)) | Err(Error::ConnectionClosed | Error::AlreadyClosed) => Some(Ending::Closed),
            Ok(_) => {
                self.heard = Instant::now();
                None
            }
            Err(Error::Capacity(_)) => Some(Ending::Refused("a message passed 1 MiB".into())),
            Err(e) if waiting(&e) => None,
            Err(_) => Some(Ending::Lost),
        }
    }

    fn release_held(&mut self) {
        while !self.running.blocked() {
            let Some(input) = self.held.pop_front() else { return };
            self.held_bytes -= input.len();
            self.running.write(input);
        }
    }

    fn deliver(&mut self, input: Result<Input, String>) -> Option<Ending> {
        match input {
            Ok(input) if !self.held.is_empty() || self.running.blocked() => {
                self.held_bytes += input.len();
                self.held.push_back(input);
                None
            }
            Ok(input) => {
                if self.running.write(input) == Writing::Gone {
                    log::debug!("web", "phone app input the command no longer reads");
                }
                None
            }
            Err(text) => Some(Ending::Refused(text)),
        }
    }

    fn speak(&mut self) -> Option<Ending> {
        if self.flushing {
            match self.flushed() {
                Some(true) => {}
                Some(false) => return None,
                None => return Some(Ending::Lost),
            }
        }
        for _ in 0..WRITES_PER_TURN {
            let Some(out) = self.running.next() else { break };
            match out {
                Out::TooLong => return Some(Ending::Refused("a line of output passed 16 MiB".into())),
                Out::End => {}
                Out::Line(json) => match self.ws.write(Message::text(json)) {
                    Ok(()) => {}
                    Err(e) if waiting(&e) => {
                        self.flushing = true;
                        return None;
                    }
                    Err(_) => return Some(Ending::Lost),
                },
            }
        }
        self.flushed().is_none().then_some(Ending::Lost)
    }

    fn flushed(&mut self) -> Option<bool> {
        match self.ws.flush() {
            Ok(()) => {
                self.flushing = false;
                Some(true)
            }
            Err(e) if waiting(&e) => {
                self.flushing = true;
                Some(false)
            }
            Err(_) => None,
        }
    }

    fn keep_alive(&mut self) -> Option<Ending> {
        let limits = &self.context.limits;
        if self.pinged.elapsed() >= limits.ping_every {
            self.pinged = Instant::now();
            match self.ws.write(Message::Ping(Bytes::new())) {
                Err(e) if !waiting(&e) => return Some(Ending::Lost),
                _ => self.flushing = true,
            }
        }
        (self.heard.elapsed() >= limits.silent_for).then_some(Ending::Silent)
    }

    fn conclude(&mut self, ending: &Ending) {
        if !matches!(ending, Ending::Exited(_)) {
            self.running.kill();
        }
        match ending {
            Ending::Exited(code) => {
                let _ = self.ws.get_ref().tcp().set_write_timeout(Some(WRITE_TIMEOUT));
                if tell(&mut self.ws, &Output::Exit(*code)) {
                    finish(&mut self.ws, CloseCode::Normal);
                    linger(&mut self.ws, self.context.limits.linger);
                }
                self.ws.get_mut().end();
            }
            Ending::Refused(text) => {
                let _ = self.ws.get_ref().tcp().set_write_timeout(Some(WRITE_TIMEOUT));
                refuse(&mut self.ws, text, self.context.limits.linger);
            }
            Ending::Stopped => {
                stopped(&mut self.ws);
            }
            Ending::Closed | Ending::Lost | Ending::Silent => {
                let _ = self.ws.flush();
            }
        }
    }
}
