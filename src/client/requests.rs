use std::io::{self, Write};
use std::os::fd::BorrowedFd;
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
use crate::remote::Remote;
use crate::update::restart;

use super::{Link, Source};

static VIEW: OnceLock<u64> = OnceLock::new();
static REMOTE: OnceLock<Remote> = OnceLock::new();

pub(crate) fn use_remote(remote: Remote) {
    let _ = REMOTE.set(remote);
}

pub(crate) fn used_remote() -> Option<&'static Remote> {
    REMOTE.get()
}

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

fn require_on(link: Result<Link>, feature: &str, lacks: &str) -> Result<()> {
    let lacking = || {
        Error::Control(format!(
            "the running cornercase server {lacks}; update cornercase and restart the server (this one is {})",
            crate::version::CURRENT
        ))
    };
    let asked =
        link.and_then(|mut link| ask_on(&mut link, "contract", control::Command::Contract(control::Contract {})));
    let asked = asked.map_err(|e| if matches!(e, Error::Control(_) | Error::OldServer(_)) { lacking() } else { e });
    let descriptor: control::contract::Descriptor = answer(asked?)?;
    if descriptor.features.iter().any(|offered| offered == feature) { Ok(()) } else { Err(lacking()) }
}

fn connect() -> Result<Link> {
    connect_within(None)
}

fn connect_within(timeout: Option<Duration>) -> Result<Link> {
    if let Some(remote) = used_remote() {
        return Link::remote(remote, true);
    }
    let stream = super::running()?;
    stream.set_read_timeout(timeout)?;
    Link::socket(stream)
}

pub(crate) fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    ask_on(&mut connect()?, name, command)
}

pub(crate) fn ask_starting(name: &'static str, command: control::Command) -> Result<Value> {
    let mut link = match used_remote() {
        Some(remote) => Link::remote(remote, true)?,
        None => Link::socket(super::find_or_start()?.stream)?,
    };
    ask_on(&mut link, name, command)
}

fn lost(link: &mut Link, local: Error) -> Error {
    let guard = &mut link.guard;
    match (&guard.remote, guard.child.as_mut(), &guard.stderr) {
        (Some(remote), Some(child), Some(stderr)) => {
            remote.ended(child, stderr, guard.collector.take()).unwrap_or(local)
        }
        _ => local,
    }
}

fn ask_on(link: &mut Link, name: &'static str, command: control::Command) -> Result<Value> {
    if let Err(e) = request(&mut link.writer, command) {
        return Err(lost(link, e));
    }
    loop {
        match protocol::recv::<ServerMessage>(&mut link.reader) {
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
            Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_))) => return Err(Error::ServerGone),
            Ok(None) | Err(_) => return Err(lost(link, Error::ServerGone)),
        }
    }
}

pub(crate) fn request(stream: &mut impl Write, command: control::Command) -> Result<()> {
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
    let mut link = connect()?;
    let answer = ask_on(&mut link, name, command)?;
    if first && each(answer)? {
        return Ok(());
    }
    match follow_on(&mut link.reader, watched(rustix::stdio::stdout()), each) {
        Err(Error::Control(message)) if message == LOST => Err(lost(&mut link, Error::Control(message))),
        followed => followed,
    }
}

fn follow_on(
    mut stream: impl Source,
    mut out: Option<BorrowedFd<'_>>,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    loop {
        if reader_left(&stream, &mut out) {
            stream.close();
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

fn reader_left(stream: &impl Source, out: &mut Option<BorrowedFd<'_>>) -> bool {
    if stream.buffered() {
        return false;
    }
    while let Some(reader) = *out {
        let server = stream.fd();
        let mut fds = [PollFd::new(&server, PollFlags::IN), PollFd::new(&reader, PollFlags::IN | PollFlags::HUP)];
        match rustix::event::poll(&mut fds, None) {
            Ok(_) => {}
            Err(Errno::INTR) => continue,
            Err(_) => return false,
        }
        let (heard, left) = (fds[0].revents(), fds[1].revents());
        if left.intersects(PollFlags::HUP | PollFlags::ERR) {
            return true;
        }
        if !left.is_empty() {
            *out = None;
        }
        if !heard.is_empty() {
            return false;
        }
    }
    false
}

pub(crate) fn answer<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::Control(format!("cannot read the server's answer: {e}")))
}

pub(crate) fn report_now() -> Option<control::Report> {
    let mut link = connect_within(Some(STATUS_TIMEOUT)).ok()?;
    let value = ask_on(&mut link, "status", control::Command::Status(control::Status {})).ok()?;
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
