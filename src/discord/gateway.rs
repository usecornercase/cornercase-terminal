use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::ops::RangeInclusive;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::{Bot, Failure};
use crate::log;
use crate::messenger::link::{Event, Link, MOST_WAIT, backoff, nap};
use crate::messenger::{Update, Via};
use crate::panics;

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

const DIRECT_MESSAGES: u64 = 1 << 12;
const FORUM_INTENTS: u64 = DIRECT_MESSAGES | 1 << 9 | 1 << 15;
const VIA: Via = Via::Discord;
const READ: Duration = Duration::from_millis(100);
const HANDSHAKE: Duration = Duration::from_secs(20);
const WRITE: Duration = Duration::from_secs(10);
const QUERY: &str = "v=10&encoding=json";
const ANEW: Duration = Duration::from_secs(2);
const AUTHENTICATION_FAILED: u16 = 4004;
const FATAL: RangeInclusive<u16> = 4010..=4014;
const DISALLOWED_INTENTS: u16 = 4014;
pub(crate) const DIRECT_ONLY: &str =
    "the bot cannot read the forum: turn on Message Content Intent in the Developer Portal, on the Bot page";
const FORGET: [u16; 2] = [4007, 4009];
const KEEP_SESSION: u16 = 4000;

pub(crate) fn start<E: From<(Via, Event)> + Send + 'static>(
    id: u64,
    bot: &Bot,
    tx: &Sender<E>,
    since: i64,
    forum: Option<i64>,
) -> Link {
    let mut listener =
        Listener { id, bot: bot.clone(), tx: tx.clone(), since, forum, session: None, seq: None, failed: 0 };
    Link::start(id, VIA, tx, bot.clone(), move |stop| listener.listen(stop))
}

struct Session {
    id: String,
    resume: String,
}

enum End {
    Stopped,
    Rejected,
    Fatal(String),
    DirectOnly,
    Resume,
    Anew,
    Failed(String),
}

struct Beat {
    every: Option<Duration>,
    at: Instant,
    acked: bool,
}

struct Listener<E> {
    id: u64,
    bot: Bot,
    tx: Sender<E>,
    since: i64,
    forum: Option<i64>,
    session: Option<Session>,
    seq: Option<i64>,
    failed: u32,
}

fn with_query(url: &str) -> String {
    match url.split_once('?') {
        Some(_) => format!("{url}&{QUERY}"),
        None if url.ends_with('/') => format!("{url}?{QUERY}"),
        None => format!("{url}/?{QUERY}"),
    }
}

fn tcp(socket: &Socket) -> Option<&TcpStream> {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => Some(stream),
        MaybeTlsStream::Rustls(stream) => Some(stream.get_ref()),
        _ => None,
    }
}

fn connect(url: &str) -> Result<Socket, String> {
    let request = with_query(url).into_client_request().map_err(|e| format!("a bad Discord gateway address: {e}"))?;
    let uri = request.uri();
    let host = uri.host().unwrap_or_default().to_string();
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("ws") { 80 } else { 443 });
    let addresses = (host.as_str(), port).to_socket_addrs().map_err(|e| format!("could not find {host}: {e}"))?;
    let stream = addresses
        .into_iter()
        .find_map(|address| TcpStream::connect_timeout(&address, HANDSHAKE).ok())
        .ok_or_else(|| format!("could not reach the Discord gateway at {host}"))?;
    stream.set_read_timeout(Some(HANDSHAKE)).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(WRITE)).map_err(|e| e.to_string())?;
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, None, None)
        .map_err(|e| format!("could not open the Discord gateway: {e}"))?;
    if let Some(stream) = tcp(&socket) {
        stream.set_read_timeout(Some(READ)).map_err(|e| e.to_string())?;
    }
    Ok(socket)
}

fn waiting(error: &tungstenite::Error) -> bool {
    matches!(error, tungstenite::Error::Io(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

fn jitter(every: Duration) -> Duration {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    every.mul_f64(f64::from(nanos % 1000) / 1000.0)
}

fn closed(code: Option<u16>, forum: bool) -> End {
    match code {
        Some(AUTHENTICATION_FAILED) => End::Rejected,
        Some(DISALLOWED_INTENTS) if forum => End::DirectOnly,
        Some(code) if FATAL.contains(&code) => {
            End::Fatal(format!("the Discord gateway refused this bot (code {code}): check the app's settings"))
        }
        Some(code) if FORGET.contains(&code) => End::Anew,
        Some(code) => End::Failed(format!("the Discord gateway closed the connection (code {code})")),
        None => End::Failed("the Discord gateway closed the connection".into()),
    }
}

impl<E: From<(Via, Event)>> Listener<E> {
    fn tell(&self, event: Event) -> bool {
        self.tx.send((VIA, event).into()).is_ok()
    }

    fn listen(&mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            let end = panics::job(|| Ok(self.connect_and_run(stop))).unwrap_or_else(|e| End::Failed(e.to_string()));
            if !self.settle(end, stop) {
                return;
            }
        }
    }

    fn settle(&mut self, end: End, stop: &AtomicBool) -> bool {
        match end {
            End::Stopped => return false,
            End::Rejected => {
                log::warning!("discord", "token refused", link = self.id);
                let _ = self.tell(Event::Rejected(self.id));
                return false;
            }
            End::Fatal(why) => {
                log::warning!("discord", "gateway refused", link = self.id, error = why);
                let _ = self.tell(Event::Problem(self.id, Some(why)));
                return false;
            }
            End::DirectOnly => return self.direct_only(stop),
            End::Resume => {}
            End::Anew => {
                self.session = None;
                self.seq = None;
                nap(stop, ANEW);
            }
            End::Failed(why) => return self.fail(why, stop),
        }
        true
    }

    fn direct_only(&mut self, stop: &AtomicBool) -> bool {
        log::warning!("discord", "forum intents refused", link = self.id);
        self.forum = None;
        self.session = None;
        self.seq = None;
        if !self.tell(Event::DirectOnly(self.id)) {
            return false;
        }
        nap(stop, ANEW);
        true
    }

    fn fail(&mut self, why: String, stop: &AtomicBool) -> bool {
        self.failed += 1;
        if self.failed.is_power_of_two() {
            log::warning!("discord", "cannot read messages", link = self.id, failed = self.failed, error = why);
        }
        if !self.tell(Event::Problem(self.id, Some(why))) {
            return false;
        }
        nap(stop, backoff(self.failed));
        true
    }

    fn connect_and_run(&mut self, stop: &AtomicBool) -> End {
        let url = match &self.session {
            Some(session) => session.resume.clone(),
            None => match self.bot.gateway() {
                Ok(url) => url,
                Err(Failure::Rejected) => return End::Rejected,
                Err(Failure::Wait(seconds)) => {
                    nap(stop, Duration::from_secs(seconds.min(MOST_WAIT)));
                    return End::Resume;
                }
                Err(failure) => return End::Failed(failure.to_string()),
            },
        };
        if stop.load(Ordering::Relaxed) {
            return End::Stopped;
        }
        let mut socket = match connect(&url) {
            Ok(socket) => socket,
            Err(why) => return End::Failed(why),
        };
        let end = self.run(&mut socket, &url, stop);
        let code = if matches!(end, End::Stopped | End::Anew) { CloseCode::Normal } else { KEEP_SESSION.into() };
        let _ = socket.close(Some(CloseFrame { code, reason: "".into() }));
        let _ = socket.flush();
        end
    }

    fn run(&mut self, socket: &mut Socket, url: &str, stop: &AtomicBool) -> End {
        let mut beat = Beat { every: None, at: Instant::now(), acked: true };
        loop {
            if stop.load(Ordering::Relaxed) {
                return End::Stopped;
            }
            if let Some(end) = self.heartbeat(socket, &mut beat) {
                return end;
            }
            if let Some(end) = self.read(socket, url, &mut beat) {
                return end;
            }
        }
    }

    fn heartbeat(&self, socket: &mut Socket, beat: &mut Beat) -> Option<End> {
        let every = beat.every.filter(|_| Instant::now() >= beat.at)?;
        if !beat.acked {
            return Some(End::Failed("the Discord gateway stopped answering".into()));
        }
        if let Err(why) = send(socket, &json!({ "op": 1, "d": self.seq })) {
            return Some(End::Failed(why));
        }
        beat.acked = false;
        beat.at = Instant::now() + every;
        None
    }

    fn read(&mut self, socket: &mut Socket, url: &str, beat: &mut Beat) -> Option<End> {
        match socket.read() {
            Ok(Message::Text(text)) => {
                let payload: Value = serde_json::from_str(text.as_str()).unwrap_or(Value::Null);
                self.payload(socket, url, &payload, beat)
            }
            Ok(Message::Close(frame)) => Some(closed(frame.map(|frame| u16::from(frame.code)), self.forum.is_some())),
            Ok(_) => None,
            Err(e) if waiting(&e) => None,
            Err(e) => Some(End::Failed(format!("the Discord gateway failed: {e}"))),
        }
    }

    fn payload(&mut self, socket: &mut Socket, url: &str, payload: &Value, beat: &mut Beat) -> Option<End> {
        match payload["op"].as_u64() {
            Some(0) => {
                if let Some(seq) = payload["s"].as_i64() {
                    self.seq = Some(seq);
                }
                self.dispatch(url, payload["t"].as_str().unwrap_or_default(), &payload["d"])
            }
            Some(1) => send(socket, &json!({ "op": 1, "d": self.seq })).err().map(End::Failed),
            Some(7) => Some(End::Resume),
            Some(9) if payload["d"].as_bool() == Some(true) && self.session.is_some() => Some(End::Resume),
            Some(9) => Some(End::Anew),
            Some(10) => {
                let every = Duration::from_millis(payload["d"]["heartbeat_interval"].as_u64().unwrap_or(41_250));
                *beat = Beat { every: Some(every), at: Instant::now() + jitter(every), acked: true };
                send(socket, &self.hello()).err().map(End::Failed)
            }
            Some(11) => {
                beat.acked = true;
                None
            }
            _ => None,
        }
    }

    fn hello(&self) -> Value {
        match (&self.session, self.seq) {
            (Some(session), Some(seq)) => json!({ "op": 6, "d": {
                "token": self.bot.token, "session_id": session.id, "seq": seq,
            }}),
            _ => json!({ "op": 2, "d": {
                "token": self.bot.token,
                "intents": if self.forum.is_some() { FORUM_INTENTS } else { DIRECT_MESSAGES },
                "properties": { "os": std::env::consts::OS, "browser": "cornercase", "device": "cornercase" },
            }}),
        }
    }

    fn dispatch(&mut self, url: &str, kind: &str, d: &Value) -> Option<End> {
        match kind {
            "READY" => {
                let resume = d["resume_gateway_url"].as_str().filter(|u| !u.is_empty()).unwrap_or(url);
                let id = d["session_id"].as_str().unwrap_or_default().to_string();
                self.session = Some(Session { id, resume: resume.to_string() });
                log::info!("discord", "gateway ready", link = self.id);
                return self.recovered();
            }
            "RESUMED" => {
                log::info!("discord", "gateway resumed", link = self.id);
                return self.recovered();
            }
            _ => {}
        }
        let update = super::update(kind, d, self.forum)?;
        if let Update::Press(press) = &update
            && let Err(failure) = self.bot.ack(&press.id)
        {
            log::warning!("discord", "button not acknowledged", link = self.id, error = failure);
        }
        let invoked = super::invoked(kind, d).filter(|invoked| match self.bot.acknowledge(invoked) {
            Ok(()) => true,
            Err(failure) => {
                log::warning!("discord", "command not acknowledged", link = self.id, error = failure);
                false
            }
        });
        let end = if update.date().is_some_and(|date| date < self.since) {
            None
        } else {
            (!self.tell(Event::Updates(self.id, vec![update]))).then_some(End::Stopped)
        };
        if let Some(invoked) = invoked
            && let Err(failure) = self.bot.dismiss(&invoked)
        {
            log::warning!("discord", "command reply not removed", link = self.id, error = failure);
        }
        end
    }

    fn recovered(&mut self) -> Option<End> {
        if self.failed == 0 {
            return None;
        }
        self.failed = 0;
        log::info!("discord", "reached again", link = self.id);
        (!self.tell(Event::Problem(self.id, None))).then_some(End::Stopped)
    }
}

fn send(socket: &mut Socket, payload: &Value) -> Result<(), String> {
    socket.send(Message::text(payload.to_string())).map_err(|e| format!("the Discord gateway failed: {e}"))
}

#[cfg(test)]
mod tests;
