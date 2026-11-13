use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use serde::Serialize;

use crate::cli::args::tunnels::HoldArg;
use crate::cli::quietly;
use crate::client;
use crate::control::{Command, Direction, TunnelFailed, TunnelHold};
use crate::error::{Error, Result};
use crate::os::process::on_end_signal;
use crate::remote::tunnel::{self, Forward, Reverse, Running, Watched};

const NEEDS_REMOTE: &str =
    "`tunnel hold` needs --remote DESTINATION: it holds a tunnel to that machine from this computer";
const NEEDS_A_TUNNEL: &str = "`tunnel hold` needs a PORT, or --reverse LOCAL";

enum Heard {
    Up,
    Registered,
    Ended(String),
    Signal,
    Stream(Result<()>),
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
    let result =
        if wait_up(&heard, hold, |reason| hold.report(reason))? { serve(&heard, tell, hold) } else { Ok(true) };
    drop(running);
    result
}

fn reverse(destination: &str, local: u16, there: Option<u16>) -> Result<bool> {
    client::use_tunnel_reverse()?;
    let asked = Held { port: there.unwrap_or(0), local, direction: Some(Direction::Reverse) };
    let (tell, heard) = channel()?;
    let wanted = Reverse { local, there: asked.port };
    let running = tunnel::spawn_reverse(destination, wanted, watcher(&tell))?;
    let result = reversed(&heard, tell, &running, asked);
    drop(running);
    result
}

fn reversed(heard: &Receiver<Heard>, tell: Sender<Heard>, running: &Running, asked: Held) -> Result<bool> {
    if !wait_up(heard, asked, |_| {})? {
        return Ok(true);
    }
    let Some(port) = running.there() else {
        return Err(asked.failed("ssh did not say which port the machine listens on"));
    };
    serve(heard, tell, Held { port, ..asked })
}

fn serve(heard: &Receiver<Heard>, tell: Sender<Heard>, hold: Held) -> Result<bool> {
    let registered = tell.clone();
    thread::spawn(move || {
        let said = client::follow_all("tunnel-hold", hold.command(), |_| {
            let _ = registered.send(Heard::Registered);
            Ok(false)
        });
        let _ = tell.send(Heard::Stream(said));
    });
    if !wait_registered(heard, hold, |reason| hold.report(reason))? || announce(&mut io::stdout().lock(), hold)? {
        return Ok(true);
    }
    held(heard, hold)
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
            Heard::Stream(_) | Heard::Registered => {}
        }
    }
}

fn wait_registered(heard: &Receiver<Heard>, hold: Held, report: impl FnOnce(&str)) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Registered => return Ok(true),
            Heard::Signal | Heard::Stream(Ok(())) => return Ok(false),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => {
                report(&reason);
                return Err(hold.ended(&reason));
            }
            Heard::Up => {}
        }
    }
}

fn held(heard: &Receiver<Heard>, hold: Held) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Signal | Heard::Stream(Ok(())) => return Ok(true),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => return Err(hold.ended(&reason)),
            Heard::Up | Heard::Registered => {}
        }
    }
}

fn announce(out: &mut impl Write, hold: Held) -> Result<bool> {
    let text = serde_json::to_string(&hold.line()).map_err(|error| Error::Control(error.to_string()))?;
    quietly(writeln!(out, "{text}").and_then(|()| out.flush()))
}

#[cfg(test)]
mod tests;
