use std::io::{self};
use std::net::Shutdown;
use std::os::fd::BorrowedFd;
use std::os::unix::net::UnixStream;
use std::sync::OnceLock;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags};
use rustix::fs::FileType;
use rustix::io::Errno;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::control::{self, Request, Response};
use crate::error::{Error, Result};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::update::restart;

static VIEW: OnceLock<u64> = OnceLock::new();

pub(crate) fn use_view(view: u64) -> Result<()> {
    require("views", "has no views")?;
    let _ = VIEW.set(view);
    Ok(())
}

pub(crate) fn use_host_events() -> Result<()> {
    require("host-events", "cannot send programs' clipboard writes and bells")
}

pub(crate) fn use_notices() -> Result<()> {
    require("notices", "cannot send agent notices or tell a hidden view apart")
}

pub(crate) fn use_surface_patches() -> Result<()> {
    require("surface-patches", "cannot send surface patches")
}

pub(crate) fn use_popup_terminal() -> Result<()> {
    require("popup-terminal", "cannot stream or type into a plugin popup")
}

pub(crate) fn use_close_unlock() -> Result<()> {
    require("close-unlock", "cannot unlock a worktree to remove it")
}

pub(crate) fn use_focus_reveal() -> Result<()> {
    require("focus-reveal", "cannot reveal a search match in its pane")
}

pub(crate) fn use_close_agent_tabs() -> Result<()> {
    require("close-agent-tabs", "would remove an agent's own worktree when it closes the agent")
}

pub(crate) fn use_issues() -> Result<()> {
    require("issues", "cannot start work from an issue")
}

pub(crate) fn use_issue_names() -> Result<()> {
    require("issue-names", "cannot name the workspace an issue starts in")
}

pub(crate) fn use_todo_panel() -> Result<()> {
    require("todo-panel", "cannot edit, uncheck, reorder or restore TODO items")
}

pub(crate) fn use_queue_undo() -> Result<()> {
    require("queue-undo", "cannot undo queue removals or clear started entries")
}

pub(crate) fn use_agent_folds() -> Result<()> {
    require("agent-folds", "cannot collapse or expand an agent's children")
}

pub(crate) fn use_accounts() -> Result<()> {
    require("accounts", "has no account profiles")
}

pub(crate) fn use_account_login() -> Result<()> {
    require("account-login", "cannot check the accounts' logins")
}

pub(crate) fn use_account_switch() -> Result<()> {
    require("account-switch", "cannot rename or switch accounts")
}

pub(crate) fn use_account_restart() -> Result<()> {
    require("account-restart", "cannot restart agents on another account")
}

fn require(feature: &str, lacks: &str) -> Result<()> {
    require_on(connect(), feature, lacks)
}

fn require_on(stream: Result<UnixStream>, feature: &str, lacks: &str) -> Result<()> {
    let lacking = || {
        Error::Control(format!(
            "the running cornercase server {lacks}; update cornercase and restart the server (this one is {})",
            crate::version::CURRENT
        ))
    };
    let asked = stream.and_then(|stream| ask_on(stream, "contract", control::Command::Contract(control::Contract {})));
    let descriptor: control::contract::Descriptor = answer(asked.map_err(|_| lacking())?)?;
    if descriptor.features.iter().any(|offered| offered == feature) { Ok(()) } else { Err(lacking()) }
}

pub(crate) fn connect() -> Result<UnixStream> {
    super::find()?.map(|found| found.stream).ok_or(Error::NoServer)
}

pub(crate) fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    ask_on(connect()?, name, command)
}

pub(crate) fn ask_starting(name: &'static str, command: control::Command) -> Result<Value> {
    ask_on(super::find_or_start()?.stream, name, command)
}

fn ask_on(mut stream: UnixStream, name: &'static str, command: control::Command) -> Result<Value> {
    request(&mut stream, command)?;
    loop {
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => return answered(&text),
            Ok(Some(ServerMessage::Rejected(_))) => return Err(Error::OldServer(name)),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => return Err(Error::OldServer(name)),
            Ok(Some(
                ServerMessage::Frame(_)
                | ServerMessage::Detached
                | ServerMessage::Sound(_)
                | ServerMessage::Open(_)
                | ServerMessage::Browse(_),
            )) => {}
            Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_)) | None) | Err(_) => {
                return Err(Error::ServerGone);
            }
        }
    }
}

pub(crate) fn request(stream: &mut UnixStream, command: control::Command) -> Result<()> {
    let caller = std::env::var(control::PANE_ENV).ok().and_then(|id| id.parse().ok());
    let server = std::env::var(control::SERVER_ENV).ok();
    let view = view_for(&command, VIEW.get().copied());
    let request = serde_json::to_string(&Request { caller, server, contract: None, boot: None, view, command })
        .map_err(|e| Error::Control(e.to_string()))?;
    protocol::send(stream, &ClientMessage::Request(request))?;
    Ok(())
}

fn view_for(command: &control::Command, view: Option<u64>) -> Option<u64> {
    view.filter(|_| !matches!(command, control::Command::Contract(_)))
}

pub(crate) fn answered(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(Response::Ok(value)) => Ok(value),
        Ok(Response::Error(message)) => Err(Error::Control(message)),
        Err(e) => Err(Error::Control(format!("cannot read the server's answer: {e}"))),
    }
}

pub(crate) fn follow(
    name: &'static str,
    command: control::Command,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    follow_from(name, command, false, each)
}

pub(crate) fn follow_all(
    name: &'static str,
    command: control::Command,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    follow_from(name, command, true, each)
}

fn follow_from(
    name: &'static str,
    command: control::Command,
    first: bool,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    let stream = connect()?;
    let reader = stream.try_clone()?;
    let answer = ask_on(reader, name, command)?;
    if first && each(answer)? {
        return Ok(());
    }
    follow_on(stream, watched(rustix::stdio::stdout()), each)
}

fn follow_on(
    mut stream: UnixStream,
    mut out: Option<BorrowedFd<'_>>,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    loop {
        if reader_left(&stream, &mut out) {
            let _ = stream.shutdown(Shutdown::Both);
            return Ok(());
        }
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                if each(answered(&text)?)? {
                    return Ok(());
                }
            }
            Ok(Some(ServerMessage::Detached | ServerMessage::Shutdown | ServerMessage::Restart(_))) => return Ok(()),
            Ok(Some(
                ServerMessage::Frame(_)
                | ServerMessage::Rejected(_)
                | ServerMessage::Sound(_)
                | ServerMessage::Open(_)
                | ServerMessage::Browse(_),
            )) => {}
            Ok(None) | Err(_) => return Err(Error::Control(LOST.into())),
        }
    }
}

fn watched(out: BorrowedFd<'_>) -> Option<BorrowedFd<'_>> {
    let kind = rustix::fs::fstat(out).map(|stat| FileType::from_raw_mode(stat.st_mode));
    matches!(kind, Ok(FileType::Fifo | FileType::Socket)).then_some(out)
}

fn reader_left(stream: &UnixStream, out: &mut Option<BorrowedFd<'_>>) -> bool {
    while let Some(reader) = *out {
        let mut fds = [PollFd::new(stream, PollFlags::IN), PollFd::new(&reader, PollFlags::IN | PollFlags::HUP)];
        match rustix::event::poll(&mut fds, None) {
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(_) => return false,
        }
        let (server, left) = (fds[0].revents(), fds[1].revents());
        if left.intersects(PollFlags::HUP | PollFlags::ERR) {
            return true;
        }
        if !left.is_empty() {
            *out = None;
        }
        if !server.is_empty() {
            return false;
        }
    }
    false
}

pub(crate) fn answer<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Control(format!("cannot read the server's answer: {e}")))
}

pub(crate) fn report_now() -> Option<control::Report> {
    let stream = connect().ok()?;
    stream.set_read_timeout(Some(STATUS_TIMEOUT)).ok()?;
    let value = ask_on(stream, "status", control::Command::Status(control::Status {})).ok()?;
    answer(value).ok()
}

pub(crate) fn running_now() -> Option<Vec<restart::Running>> {
    report_now().map(|report| restart::from_report(&report))
}
const STATUS_TIMEOUT: Duration = Duration::from_secs(1);
const LOST: &str = "the cornercase server closed the connection without stopping: it crashed, or this command \
    stopped reading what it sends";

#[cfg(test)]
mod tests;
