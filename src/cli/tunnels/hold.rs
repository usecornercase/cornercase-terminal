use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde::Serialize;

use crate::cli::quietly;
use crate::client;
use crate::control::{Command, TunnelFailed, TunnelHold};
use crate::error::{Error, Result};
use crate::os::process::on_end_signal;
use crate::remote::tunnel::{self, Forward, Watched};

const NEEDS_REMOTE: &str =
    "`tunnel hold` needs --remote DESTINATION: it holds a tunnel to that machine from this computer";

enum Heard {
    Up,
    Registered,
    Ended(String),
    Signal,
    Stream(Result<()>),
}

#[derive(Serialize)]
struct Line {
    port: u16,
    local: u16,
    url: String,
}

pub(super) fn run(port: u16) -> Result<bool> {
    client::own_connection()?;
    let remote = client::used_remote().ok_or_else(|| Error::WrongUsage(NEEDS_REMOTE.into()))?;
    client::use_tunnel_hold()?;
    let local = tunnel::local_port(port, None).inspect_err(|error| report(port, &error.to_string()))?;
    let (tell, heard) = mpsc::channel();
    let signalled = tell.clone();
    on_end_signal(move || {
        let _ = signalled.send(Heard::Signal);
    })
    .map_err(Error::Signals)?;
    let watching = tell.clone();
    let running = tunnel::spawn(&remote.destination, Forward { port, local }, move |watched| {
        let _ = watching.send(match watched {
            Watched::Up => Heard::Up,
            Watched::Ended(reason) => Heard::Ended(reason),
        });
    })
    .inspect_err(|error| report(port, &error.to_string()))?;
    let held = hold(&heard, tell, port, local);
    drop(running);
    held
}

fn hold(heard: &Receiver<Heard>, tell: mpsc::Sender<Heard>, port: u16, local: u16) -> Result<bool> {
    if !wait_up(heard, port, |reason| report(port, reason))? {
        return Ok(true);
    }
    let registered = tell.clone();
    thread::spawn(move || {
        let command = Command::TunnelHold(TunnelHold { port, local });
        let said = client::follow_all("tunnel-hold", command, |_| {
            let _ = registered.send(Heard::Registered);
            Ok(false)
        });
        let _ = tell.send(Heard::Stream(said));
    });
    if !wait_registered(heard, port, |reason| report(port, reason))? || announce(&mut io::stdout().lock(), port, local)?
    {
        return Ok(true);
    }
    held(heard, port)
}

fn report(port: u16, reason: &str) {
    let error = if reason.trim().is_empty() { "the tunnel could not be held" } else { reason };
    let command = Command::TunnelFailed(TunnelFailed { port, error: error.into() });
    let _ = client::ask("tunnel-failed", command);
}

fn wait_up(heard: &Receiver<Heard>, port: u16, report: impl FnOnce(&str)) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Up => return Ok(true),
            Heard::Signal => return Ok(false),
            Heard::Ended(reason) => {
                report(&reason);
                return Err(Error::Control(format!("could not tunnel port {port}: {reason}")));
            }
            Heard::Stream(_) | Heard::Registered => {}
        }
    }
}

fn wait_registered(heard: &Receiver<Heard>, port: u16, report: impl FnOnce(&str)) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Registered => return Ok(true),
            Heard::Signal | Heard::Stream(Ok(())) => return Ok(false),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => {
                report(&reason);
                return Err(Error::Control(format!("the tunnel of port {port} ended: {reason}")));
            }
            Heard::Up => {}
        }
    }
}

fn held(heard: &Receiver<Heard>, port: u16) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Signal | Heard::Stream(Ok(())) => return Ok(true),
            Heard::Stream(Err(error)) => return Err(error),
            Heard::Ended(reason) => {
                return Err(Error::Control(format!("the tunnel of port {port} ended: {reason}")));
            }
            Heard::Up | Heard::Registered => {}
        }
    }
}

fn announce(out: &mut impl Write, port: u16, local: u16) -> Result<bool> {
    let line = Line { port, local, url: format!("http://localhost:{local}") };
    let text = serde_json::to_string(&line).map_err(|error| Error::Control(error.to_string()))?;
    quietly(writeln!(out, "{text}").and_then(|()| out.flush()))
}

#[cfg(test)]
mod tests;
