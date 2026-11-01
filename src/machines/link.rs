use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::Child;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::control::start_server::Started;
use crate::control::{self, Command, Request, Response, Snapshot, StartServer, Watch};
use crate::error::{Error, Result};
use crate::panics;
use crate::process;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::remote::{self, Killer, Remote, Stderr, Use};
use crate::text;

const PERSON: Duration = Duration::from_secs(60);
const KEPT: usize = 200;
const PATH_KEPT: usize = 4096;
const PATHS: [&str; 3] = ["path", "home", "run_dir"];
const STOPPED: &str = "the server there stopped";
const RESTARTING: &str = "the server there is restarting";
const LET_GO: &str = "the server there ended the link";
const LOST: &str = "the link ended";
const THIS_COMPUTER: &str = "that is this computer";

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Report {
    pub(crate) link: u64,
    pub(crate) event: Event,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Event {
    Linked { release: String, server_version: String },
    Seen(Box<control::Report>),
    Down { down: Down, failed: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Down {
    Unreachable(String),
    Missing,
    Older(Option<String>),
    ThisComputer,
    SameServer(u64),
}

impl Down {
    fn of(error: &Error) -> Self {
        match error {
            Error::RemoteMissing { .. } => Self::Missing,
            Error::RemoteBehind(behind) => Self::Older(Some(clean(&behind.there))),
            Error::RemoteTooOld(_) => Self::Older(None),
            error => Self::Unreachable(clean(&remote::reason(error))),
        }
    }

    fn wait(&self, failed: u32) -> Duration {
        match self {
            Self::Unreachable(_) => remote::backoff(failed),
            _ => PERSON,
        }
    }

    pub(crate) fn reason(&self, name: &str, other: Option<&str>) -> Option<String> {
        match self {
            Self::Unreachable(why) => Some(why.clone()),
            Self::Missing => Some(remote::not_found(name)),
            Self::Older(_) => None,
            Self::ThisComputer => Some(THIS_COMPUTER.into()),
            Self::SameServer(_) => Some(format!("the same server as {}", other.unwrap_or("another machine"))),
        }
    }
}

fn clean(text: &str) -> String {
    let shown: String = text
        .chars()
        .filter_map(|c| if c.is_whitespace() { Some(' ') } else { (!text::hidden(c)).then_some(c) })
        .collect();
    text::one_line(&shown).chars().take(KEPT).collect()
}

fn clean_path(text: &str) -> String {
    text.chars().filter(|&c| !text::hidden(c)).take(PATH_KEPT).collect()
}

fn cleaned(value: Value) -> Value {
    cleaned_with(value, clean)
}

fn cleaned_with(value: Value, how: fn(&str) -> String) -> Value {
    match value {
        Value::String(text) => Value::String(how(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| cleaned_with(item, how)).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| {
                    let how = if PATHS.contains(&key.as_str()) { clean_path } else { clean };
                    (clean(&key), cleaned_with(value, how))
                })
                .collect(),
        ),
        value => value,
    }
}

pub(crate) struct Line {
    pub(crate) release: String,
    pub(crate) reader: Box<dyn Read + Send>,
    pub(crate) writer: Box<dyn Write + Send>,
    pub(crate) ends: Arc<dyn Ends>,
}

pub(crate) trait Ends: Send + Sync {
    fn cut(&self);

    fn why(&self) -> Option<String>;
}

pub(crate) trait Connect: Send + 'static {
    fn connect(&self, killer: &Killer) -> Result<Line>;
}

pub(crate) fn ssh(destination: &str) -> Box<dyn Connect> {
    Box::new(Ssh(remote(destination)))
}

pub(crate) fn remote(destination: &str) -> Remote {
    Remote { destination: destination.into(), command: None, option: "" }
}

pub(crate) fn failure(error: &Error, name: &str) -> String {
    Down::of(error).reason(name, None).unwrap_or_else(|| clean(&error.to_string()))
}

struct Ssh(Remote);

impl Connect for Ssh {
    fn connect(&self, killer: &Killer) -> Result<Line> {
        let piped = self.0.connect_killable(Use::Link, killer)?;
        let ends = Arc::new(SshEnds {
            remote: self.0.clone(),
            child: Mutex::new(Some(piped.child)),
            stderr: piped.stderr,
            collector: Mutex::new(piped.collector),
        });
        Ok(Line { release: piped.release, reader: Box::new(piped.reader), writer: Box::new(piped.writer), ends })
    }
}

struct SshEnds {
    remote: Remote,
    child: Mutex<Option<Child>>,
    stderr: Stderr,
    collector: Mutex<Option<JoinHandle<()>>>,
}

impl Ends for SshEnds {
    fn cut(&self) {
        if let Some(child) = self.child.lock().as_mut() {
            let _ = child.kill();
        }
    }

    fn why(&self) -> Option<String> {
        let mut child = self.child.lock().take()?;
        let collector = self.collector.lock().take();
        self.remote.ended(&mut child, &self.stderr, collector).map(|e| remote::reason(&e))
    }
}

impl Drop for SshEnds {
    fn drop(&mut self) {
        if let Some(child) = self.child.get_mut().take() {
            process::stop(child);
        }
    }
}

#[derive(Clone)]
pub(crate) struct Servers {
    here: String,
    claimed: Arc<Mutex<HashMap<u64, String>>>,
}

impl Servers {
    pub(crate) fn new(here: &str) -> Self {
        Self { here: here.into(), claimed: Arc::default() }
    }

    fn claim(&self, link: u64, boot: &str) -> Option<Down> {
        if boot == self.here {
            return Some(Down::ThisComputer);
        }
        let mut claimed = self.claimed.lock();
        if let Some(other) = claimed.iter().find(|(other, held)| **other != link && *held == boot).map(|(id, _)| *id) {
            return Some(Down::SameServer(other));
        }
        claimed.insert(link, boot.into());
        None
    }

    fn release(&self, link: u64) {
        self.claimed.lock().remove(&link);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    Stop,
    Poke,
    Nothing,
}

#[derive(Default)]
struct Wants {
    stop: bool,
    poke: bool,
    ends: Option<Arc<dyn Ends>>,
}

#[derive(Default)]
struct Control {
    wants: Mutex<Wants>,
    wake: Condvar,
    killer: Killer,
}

impl Control {
    fn stop(&self) {
        self.ask(|wants| wants.stop = true);
        self.killer.kill();
    }

    fn poke(&self) {
        self.ask(|wants| wants.poke = true);
    }

    fn ask(&self, what: impl FnOnce(&mut Wants)) {
        let mut wants = self.wants.lock();
        what(&mut wants);
        if let Some(ends) = &wants.ends {
            ends.cut();
        }
        self.wake.notify_all();
    }

    fn stopped(&self) -> bool {
        self.wants.lock().stop
    }

    fn hold(&self, ends: &Arc<dyn Ends>) -> Asked {
        let mut wants = self.wants.lock();
        let asked = Self::asked(&mut wants);
        if asked == Asked::Nothing {
            wants.ends = Some(Arc::clone(ends));
        }
        asked
    }

    fn drop_line(&self) {
        if let Some(ends) = self.wants.lock().ends.take() {
            ends.cut();
        }
    }

    fn release(&self) -> Asked {
        let mut wants = self.wants.lock();
        wants.ends = None;
        Self::asked(&mut wants)
    }

    fn nap(&self, wait: Duration) -> Asked {
        let until = Instant::now() + wait;
        let mut wants = self.wants.lock();
        loop {
            let asked = Self::asked(&mut wants);
            if asked != Asked::Nothing || Instant::now() >= until {
                return asked;
            }
            self.wake.wait_until(&mut wants, until);
        }
    }

    fn asked(wants: &mut Wants) -> Asked {
        if wants.stop {
            Asked::Stop
        } else if std::mem::take(&mut wants.poke) {
            Asked::Poke
        } else {
            Asked::Nothing
        }
    }
}

trait Clock: Send + 'static {
    fn nap(&self, control: &Control, wait: Duration) -> Asked;
}

struct Wall;

impl Clock for Wall {
    fn nap(&self, control: &Control, wait: Duration) -> Asked {
        control.nap(wait)
    }
}

pub(crate) struct Link {
    id: u64,
    control: Arc<Control>,
}

impl Link {
    pub(crate) fn start<E>(id: u64, connector: Box<dyn Connect>, servers: &Servers, tx: &Sender<E>) -> Self
    where
        E: From<Report> + Send + 'static,
    {
        Self::spawn(id, connector, servers, tx, Box::new(Wall))
    }

    fn spawn<E>(id: u64, connector: Box<dyn Connect>, servers: &Servers, tx: &Sender<E>, clock: Box<dyn Clock>) -> Self
    where
        E: From<Report> + Send + 'static,
    {
        let control = Arc::new(Control::default());
        let running =
            Running { id, connector, servers: servers.clone(), tx: tx.clone(), clock, control: Arc::clone(&control) };
        thread::spawn(move || running.run());
        Self { id, control }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn poke(&self) {
        self.control.poke();
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.control.stop();
    }
}

enum Outcome {
    Stop,
    Again,
    Dropped(String),
    Failed(Down),
}

enum Ended {
    Refused(Down),
    Lost(Option<&'static str>),
    Dropped(Option<&'static str>),
}

pub(super) enum Heard {
    Answer(String),
    Ended(Option<&'static str>),
}

struct Running<E> {
    id: u64,
    connector: Box<dyn Connect>,
    servers: Servers,
    tx: Sender<E>,
    clock: Box<dyn Clock>,
    control: Arc<Control>,
}

impl<E: From<Report>> Running<E> {
    fn run(self) {
        loop {
            let served = panics::job(|| {
                self.serve();
                Ok(())
            });
            self.servers.release(self.id);
            if served.is_ok() {
                return;
            }
            self.control.drop_line();
            self.tell(Event::Down { down: Down::Unreachable(Error::Bug.to_string()), failed: 1 });
            if self.clock.nap(&self.control, PERSON) == Asked::Stop {
                return;
            }
        }
    }

    fn serve(&self) {
        let mut failed: u32 = 0;
        loop {
            let down = match self.attempt() {
                Outcome::Stop => return,
                Outcome::Again => {
                    failed = 0;
                    continue;
                }
                Outcome::Dropped(why) => {
                    failed = 1;
                    Down::Unreachable(why)
                }
                Outcome::Failed(down) => {
                    failed = failed.saturating_add(1);
                    down
                }
            };
            let wait = down.wait(failed);
            self.tell(Event::Down { down, failed });
            match self.clock.nap(&self.control, wait) {
                Asked::Stop => return,
                Asked::Poke => failed = 0,
                Asked::Nothing => {}
            }
        }
    }

    fn attempt(&self) -> Outcome {
        let outcome = self.try_link();
        if !matches!(outcome, Outcome::Again) {
            self.servers.release(self.id);
        }
        outcome
    }

    fn try_link(&self) -> Outcome {
        let mut line = match self.connector.connect(&self.control.killer) {
            Ok(line) => line,
            Err(error) => return Outcome::Failed(Down::of(&error)),
        };
        match self.control.hold(&line.ends) {
            Asked::Nothing => {}
            asked => {
                line.ends.cut();
                return if asked == Asked::Stop { Outcome::Stop } else { Outcome::Again };
            }
        }
        let ended = match handshake(&mut line).and_then(|started| self.claim(started)) {
            Ok(started) => {
                let release = clean(&line.release);
                self.tell(Event::Linked { release, server_version: clean(&started.version) });
                self.watch(&mut line)
            }
            Err(ended) => ended,
        };
        self.finish(&line, ended)
    }

    fn watch(&self, line: &mut Line) -> Ended {
        if let Err(ended) = ask(line, Command::Watch(Watch::default()), Ended::Dropped(None)) {
            return ended;
        }
        loop {
            match listen(&mut line.reader) {
                Heard::Answer(text) => match answered::<Snapshot>(&text) {
                    Ok(snapshot) => self.tell(Event::Seen(Box::new(seen(snapshot.report)))),
                    Err(down) => return Ended::Refused(down),
                },
                Heard::Ended(said) => return Ended::Dropped(said),
            }
        }
    }

    fn claim(&self, started: Started) -> std::result::Result<Started, Ended> {
        match self.servers.claim(self.id, &started.boot) {
            Some(down) => Err(Ended::Refused(down)),
            None => Ok(started),
        }
    }

    fn finish(&self, line: &Line, ended: Ended) -> Outcome {
        let asked = self.control.release();
        if asked != Asked::Nothing || matches!(ended, Ended::Refused(_)) {
            line.ends.cut();
        }
        match (asked, ended) {
            (Asked::Stop, _) => Outcome::Stop,
            (Asked::Poke, _) => Outcome::Again,
            (Asked::Nothing, Ended::Refused(down)) => Outcome::Failed(down),
            (Asked::Nothing, Ended::Lost(said)) => Outcome::Failed(Down::Unreachable(why(line, said))),
            (Asked::Nothing, Ended::Dropped(said)) => Outcome::Dropped(why(line, said)),
        }
    }

    fn tell(&self, event: Event) {
        if !self.control.stopped() {
            let _ = self.tx.send(Report { link: self.id, event }.into());
        }
    }
}

pub(super) fn why(line: &Line, said: Option<&str>) -> String {
    let ssh = line.ends.why();
    clean(said.map(str::to_string).or(ssh).as_deref().unwrap_or(LOST))
}

fn handshake(line: &mut Line) -> std::result::Result<Started, Ended> {
    ask(line, Command::StartServer(StartServer::default()), Ended::Lost(None))?;
    match listen(&mut line.reader) {
        Heard::Answer(text) => answered(&text).map_err(Ended::Refused),
        Heard::Ended(said) => Err(Ended::Lost(said)),
    }
}

pub(super) fn send_once(line: &mut Line, command: Command) -> Heard {
    if ask(line, command, Ended::Lost(None)).is_err() {
        return Heard::Ended(None);
    }
    listen(&mut line.reader)
}

fn ask(line: &mut Line, command: Command, lost: Ended) -> std::result::Result<(), Ended> {
    let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
    let text = serde_json::to_string(&request).map_err(|e| Ended::Refused(Down::Unreachable(e.to_string())))?;
    protocol::send(&mut line.writer, &ClientMessage::Request(text)).map_err(|_| lost)
}

fn answered<T: DeserializeOwned>(text: &str) -> std::result::Result<T, Down> {
    let unreadable =
        |e: serde_json::Error| Down::Unreachable(clean(&format!("cannot read the answer from there: {e}")));
    match serde_json::from_str(text).map_err(unreadable)? {
        Response::Ok(value) => serde_json::from_value(cleaned(value)).map_err(unreadable),
        Response::Error(message) => Err(Down::Unreachable(clean(&message))),
    }
}

fn seen(report: control::Report) -> control::Report {
    control::Report { boot: report.boot, home: report.home, projects: report.projects, ..control::Report::default() }
}

fn listen(reader: &mut impl Read) -> Heard {
    loop {
        match protocol::recv::<ServerMessage>(reader) {
            Ok(Some(ServerMessage::Response(text))) => return Heard::Answer(text),
            Ok(Some(ServerMessage::Shutdown)) => return Heard::Ended(Some(STOPPED)),
            Ok(Some(ServerMessage::Restart(_))) => return Heard::Ended(Some(RESTARTING)),
            Ok(Some(ServerMessage::Detached)) => return Heard::Ended(Some(LET_GO)),
            Ok(Some(
                ServerMessage::Frame(_)
                | ServerMessage::Rejected(_)
                | ServerMessage::Sound(_)
                | ServerMessage::Open(_)
                | ServerMessage::Browse(_),
            )) => {}
            Err(e) if protocol::undecoded(&e) => {}
            Ok(None) | Err(_) => return Heard::Ended(None),
        }
    }
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
