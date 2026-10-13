use std::fs;
use std::io::{self, IsTerminal, Write, stdin, stdout};
use std::os::unix::net::UnixStream;

use crate::client::{RESTART, answer, answered, confirm, connect, request, restart_server, running_now, wait_for_exit};
use crate::error::{Error, Result};
use crate::protocol::{self, ServerMessage};
use crate::update::{self, Install, Outcome};
use crate::version::CURRENT;
use crate::{control, paths, restart};

const RESTART_WHEN_IDLE: &str = "Restart it once no agent is working? [y/N] ";
const WAITING: &str = "the server restarts once no agent is working; Ctrl+C cancels the restart";
const RESTARTED: &str = "restarted the cornercase server; your session comes back the next time cornercase starts";
const TOO_OLD_TO_WAIT: &str = "the running cornercase server is too old to wait for its agents; run \
    `cornercase restart` once none is working";
const STOPPED_INSTEAD: &str = "the cornercase server was stopped before every agent was idle, so it did not restart";
const INSIDE: &str = "This terminal is one of them, so it closes too.";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum When {
    Now,
    Idle { timeout: Option<f64> },
}

pub fn update(check_only: bool, yes: bool, when: When) -> Result<bool> {
    let url = std::env::var(update::LATEST_ENV).ok();
    if cfg!(debug_assertions) && url.is_none() {
        return Err(Error::DevelopmentBuild);
    }
    let url = url.unwrap_or_else(|| update::DEFAULT_LATEST.into());
    let target = update::target();
    let install = update::install(&fs::canonicalize(std::env::current_exe()?)?, target);
    if check_only {
        match update::check(&url, CURRENT)? {
            Some(release) => {
                let command = match install {
                    Install::Replace(_) => "cornercase update",
                    Install::Command(command) => command,
                };
                announce(&release.version, command);
            }
            None => println!("cornercase {CURRENT} is up to date"),
        }
        return Ok(true);
    }
    match update::install_latest(&url, CURRENT, &install, target)? {
        Outcome::UpToDate => println!("cornercase {CURRENT} is up to date"),
        Outcome::Manual(version, command) => {
            announce(&version, command);
            return Ok(false);
        }
        Outcome::Updated(version) => {
            println!("updated cornercase {CURRENT} → {version}");
            offer_restart(yes, when)?;
        }
    }
    Ok(true)
}

fn announce(version: &str, command: &str) {
    println!("cornercase {version} is out (you have {CURRENT}). Update it with:\n{command}");
}

fn offer_restart(yes: bool, when: When) -> Result<()> {
    if server_running()?
        && !restart_if_confirmed(Some(&format!("The running cornercase server still runs {CURRENT}.")), yes, when)?
    {
        println!(
            "the server keeps running {CURRENT}; run `cornercase kill-server` and start cornercase to use the new one"
        );
    }
    Ok(())
}

pub fn restart(yes: bool, when: When) -> Result<()> {
    if !server_running()? {
        eprintln!("no cornercase server is running");
    } else if !restart_if_confirmed(None, yes, when)? {
        let hint = if stdin().is_terminal() { "" } else { "; pass --yes to restart it without asking" };
        println!("the server keeps running{hint}");
    }
    Ok(())
}

fn server_running() -> Result<bool> {
    let path = paths::socket();
    protocol::check_socket_dir(&path)?;
    Ok(UnixStream::connect(&path).is_ok())
}

fn restart_if_confirmed(intro: Option<&str>, yes: bool, when: When) -> Result<bool> {
    let inside = std::env::var_os(protocol::NESTED_ENV).is_some();
    let lead: Vec<&str> = [intro, inside.then_some(INSIDE)].into_iter().flatten().collect();
    let running = if yes || stdin().is_terminal() { running_now() } else { None };
    let stops = restart::confirmation(running.as_deref());
    let ask = if when == When::Now { RESTART } else { RESTART_WHEN_IDLE };
    let question =
        if lead.is_empty() { format!("{stops}\n{ask}") } else { format!("{}\n{stops}\n{ask}", lead.join(" ")) };
    let confirmed = yes || (stdin().is_terminal() && confirm(&question));
    if !confirmed {
        return Ok(false);
    }
    if yes {
        println!("{stops}");
    }
    if let When::Idle { timeout } = when {
        restart_when_idle(timeout, inside)?;
        return Ok(true);
    }
    if inside {
        println!("restarting the cornercase server");
    }
    if restart_server()? {
        println!("{RESTARTED}");
        if let Some(line) = running.as_deref().and_then(restart::stopped) {
            println!("{line}");
        }
    }
    Ok(true)
}

fn restart_when_idle(timeout: Option<f64>, inside: bool) -> Result<()> {
    let mut stream = connect()?;
    request(&mut stream, control::Command::RestartWhenIdle(control::RestartWhenIdle { timeout }))?;
    println!("{WAITING}");
    let _ = stdout().flush();
    let mut report: Option<control::Report> = None;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                report = Some(answer(answered(&text).map_err(too_old_to_wait)?)?);
                if inside {
                    println!("restarting the cornercase server");
                }
            }
            Ok(Some(ServerMessage::Restart(_))) => break,
            Ok(Some(ServerMessage::Shutdown)) => return Err(Error::Control(STOPPED_INSTEAD.into())),
            Ok(Some(ServerMessage::Rejected(_))) => return Err(Error::OldServer("restart --when-idle")),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::OldServer("restart --when-idle")),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Detached)) => {}
            Ok(None) | Err(_) if report.is_some() => break,
            Ok(None) | Err(_) => return Err(Error::ServerGone),
        }
    }
    wait_for_exit(&paths::socket());
    println!("{RESTARTED}");
    if let Some(line) = report.as_ref().map(restart::from_report).as_deref().and_then(restart::stopped) {
        println!("{line}");
    }
    Ok(())
}

fn too_old_to_wait(error: Error) -> Error {
    match error {
        Error::Control(message) if message.contains("has no `restart-when-idle` command") => {
            Error::Control(TOO_OLD_TO_WAIT.into())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests;
