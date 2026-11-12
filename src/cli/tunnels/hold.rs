use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use serde::Serialize;

use crate::cli::quietly;
use crate::client;
use crate::control::{Command, TunnelHold};
use crate::error::{Error, Result};
use crate::os::process::on_end_signal;
use crate::remote::tunnel::{self, Forward, Watched};

const NEEDS_REMOTE: &str =
    "`tunnel hold` needs --remote DESTINATION: it holds a tunnel to that machine from this computer";

enum Heard {
    Up,
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
    let local = tunnel::local_port(port, None)?;
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
    })?;
    let held = hold(&heard, tell, port, local);
    drop(running);
    held
}

fn hold(heard: &Receiver<Heard>, tell: mpsc::Sender<Heard>, port: u16, local: u16) -> Result<bool> {
    if !wait_up(heard, port)? {
        return Ok(true);
    }
    if announce(&mut io::stdout().lock(), port, local)? {
        return Ok(true);
    }
    thread::spawn(move || {
        let command = Command::TunnelHold(TunnelHold { port, local });
        let _ = tell.send(Heard::Stream(client::follow("tunnel-hold", command, |_| Ok(false))));
    });
    held(heard, port)
}

fn wait_up(heard: &Receiver<Heard>, port: u16) -> Result<bool> {
    loop {
        match heard.recv().unwrap_or(Heard::Signal) {
            Heard::Up => return Ok(true),
            Heard::Signal => return Ok(false),
            Heard::Ended(reason) => return Err(Error::Control(format!("could not tunnel port {port}: {reason}"))),
            Heard::Stream(_) => {}
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
            Heard::Up => {}
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
