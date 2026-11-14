use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use serde::Serialize;
use serde_json::Value;

use crate::cli::args::tunnels::HoldArg;
use crate::cli::quietly;
use crate::client::{self, StreamEnd};
use crate::control::{Command, Direction, HoldEnd, HoldEnded, TunnelFailed, TunnelHold};
use crate::error::{Error, Result};
use crate::os::process::on_end_signal;
use crate::remote::tunnel::{self, Forward, Reverse, Running, Watched};

const NEEDS_REMOTE: &str =
    "`tunnel hold` needs --remote DESTINATION: it holds a tunnel to that machine from this computer";
const NEEDS_A_TUNNEL: &str = "`tunnel hold` needs a PORT, or --reverse LOCAL";

enum Heard {
    Up,
    Registered,
    Told(HoldEnd),
    Ended(String),
    Signal,
    Stream(Result<StreamEnd>),
}

#[derive(Debug, Clone, Copy)]
struct Held {
    port: u16,
    local: u16,
    direction: Option<Direction>,
}

#[derive(Serialize)]
struct Line {
    port: u16,
    local: u16,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    direction: Option<Direction>,
}

impl Held {
    fn reverse(self) -> bool {
        self.direction == Some(Direction::Reverse)
    }

    fn failed(self, reason: &str) -> Error {
        Error::Control(if self.reverse() {
            format!("could not hold a reverse tunnel to port {}: {reason}", self.local)
        } else {
            format!("could not tunnel port {}: {reason}", self.port)
        })
    }

    fn ended(self, reason: &str) -> Error {
        let kind = if self.reverse() { "reverse tunnel" } else { "tunnel" };
        Error::Control(format!("the {kind} of port {} ended: {reason}", self.port))
    }

    fn report(self, reason: &str) {
        if self.reverse() {
            return;
        }
        let error = if reason.trim().is_empty() { "the tunnel could not be held" } else { reason };
        let command = Command::TunnelFailed(TunnelFailed { port: self.port, error: error.into() });
        let _ = client::ask("tunnel-failed", command);
    }

    fn line(self) -> Line {
        let url = if self.reverse() {
            format!("http://127.0.0.1:{}", self.port)
        } else {
            format!("http://localhost:{}", self.local)
        };
        Line { port: self.port, local: self.local, url, direction: self.direction }
    }

    fn command(self) -> Command {
        Command::TunnelHold(TunnelHold { port: self.port, local: self.local, direction: self.direction })
    }
}

pub(super) fn run(arg: &HoldArg) -> Result<bool> {
    client::own_connection()?;
    let remote = client::used_remote().ok_or_else(|| Error::WrongUsage(NEEDS_REMOTE.into()))?;
    match (arg.reverse, arg.port) {
        (Some(local), _) => reverse(&remote.destination, local, arg.there),
        (None, Some(port)) => forward(&remote.destination, port, arg.local),
        (None, None) => Err(Error::WrongUsage(NEEDS_A_TUNNEL.into())),
    }
}

fn channel() -> Result<(Sender<Heard>, Receiver<Heard>)> {
    let (tell, heard) = mpsc::channel();
    let signalled = tell.clone();
    on_end_signal(move || {
        let _ = signalled.send(Heard::Signal);
    })
    .map_err(Error::Signals)?;
    Ok((tell, heard))
}

fn watcher(tell: &Sender<Heard>) -> impl Fn(Watched) + Send + 'static + use<> {
    let watching = tell.clone();
    move |watched| {
        let _ = watching.send(match watched {
            Watched::Up => Heard::Up,
            Watched::Ended(reason) => Heard::Ended(reason),
        });
    }
}

fn forward(destination: &str, port: u16, wanted: Option<u16>) -> Result<bool> {
    client::use_tunnel_hold()?;
    let asked = Held { port, local: port, direction: None };
    let local = tunnel::local_port(port, wanted).inspect_err(|error| asked.report(&error.to_string()))?;
    let hold = Held { local, ..asked };
    let (tell, heard) = channel()?;
    let running = tunnel::spawn(destination, Forward { port, local }, watcher(&tell))
        .inspect_err(|error| hold.report(&error.to_string()))?;
    let ended = if wait_up(&heard, hold, |reason| hold.report(reason))? { serve(&heard, tell, hold) } else { Ok(None) };
    running.finish();
    said(ended)
}

fn reverse(destination: &str, local: u16, there: Option<u16>) -> Result<bool> {
    client::use_tunnel_reverse()?;
    let asked = Held { port: there.unwrap_or(0), local, direction: Some(Direction::Reverse) };
    let (tell, heard) = channel()?;
    let wanted = Reverse { local, there: asked.port };
    let running = tunnel::spawn_reverse(destination, wanted, watcher(&tell))?;
    let ended = reversed(&heard, tell, &running, asked);
    running.finish();
    said(ended)
}

fn reversed(heard: &Receiver<Heard>, tell: Sender<Heard>, running: &Running, asked: Held) -> Result<Option<HoldEnd>> {
    if !wait_up(heard, asked, |_| {})? {
        return Ok(None);
    }
    let Some(port) = running.there() else {
        return Err(asked.failed("ssh did not say which port the machine listens on"));
    };
    serve(heard, tell, Held { port, ..asked })
}

fn heard_of(value: &Value) -> Heard {
    match value.get("ended").and_then(|ended| serde_json::from_value(ended.clone()).ok()) {
        Some(end) => Heard::Told(end),
        None => Heard::Registered,
    }
}

fn serve(heard: &Receiver<Heard>, tell: Sender<Heard>, hold: Held) -> Result<Option<HoldEnd>> {
    let registered = tell.clone();
    thread::spawn(move || {
        let said = client::follow_to_end("tunnel-hold", hold.command(), |value| {
            let _ = registered.send(heard_of(&value));
            Ok(false)
        });
        let _ = tell.send(Heard::Stream(said));
    });
    if !wait_registered(heard, hold, |reason| hold.report(reason))? || announce(&mut io::stdout().lock(), hold)? {
        return Ok(None);
    }
    held(heard, hold)
}

fn said(ended: Result<Option<HoldEnd>>) -> Result<bool> {
    let Some(end) = ended? else { return Ok(true) };
    let reader_left = say_ended(&mut io::stdout().lock(), end)?;
    status_of(end, reader_left)
}

fn status_of(end: HoldEnd, reader_left: bool) -> Result<bool> {
    if end.server_stopped() && !reader_left { Err(Error::ServerStopped) } else { Ok(true) }
}

fn wait_up(heard: &Receiver<Heard>, hold: Held, report: impl FnOnce(&str)) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Up => return Ok(true),
            Heard::Signal => return Ok(false),
            Heard::Ended(reason) => {
                report(&reason);
                return Err(hold.failed(&reason));
            }
            Heard::Stream(_) | Heard::Registered | Heard::Told(_) => {}
        }
    }
}

fn wait_registered(heard: &Receiver<Heard>, hold: Held, report: impl FnOnce(&str)) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Registered => return Ok(true),
            Heard::Signal | Heard::Stream(Ok(_)) => return Ok(false),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => {
                report(&reason);
                return Err(hold.ended(&reason));
            }
            Heard::Up | Heard::Told(_) => {}
        }
    }
}

fn held(heard: &Receiver<Heard>, hold: Held) -> Result<Option<HoldEnd>> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Told(end) => return Ok(Some(end)),
            Heard::Stream(Ok(StreamEnd::Stopped)) => return Ok(Some(HoldEnd::ServerStopped)),
            Heard::Stream(Ok(StreamEnd::Restarted)) => return Ok(Some(HoldEnd::ServerRestarted)),
            Heard::Signal | Heard::Stream(Ok(StreamEnd::Left | StreamEnd::Detached)) => return Ok(None),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => return Err(hold.ended(&reason)),
            Heard::Up | Heard::Registered => {}
        }
    }
}

fn write_line(out: &mut impl Write, value: &impl Serialize) -> Result<bool> {
    let text = serde_json::to_string(value).map_err(|error| Error::Control(error.to_string()))?;
    quietly(writeln!(out, "{text}").and_then(|()| out.flush()))
}

fn announce(out: &mut impl Write, hold: Held) -> Result<bool> {
    write_line(out, &hold.line())
}

fn say_ended(out: &mut impl Write, end: HoldEnd) -> Result<bool> {
    write_line(out, &HoldEnded { ended: end })
}

#[cfg(test)]
mod tests;
