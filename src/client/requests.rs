use std::io::{self, Write};
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::control::{self, Request};
use crate::error::{Error, Result};
use crate::os::console::{self, BorrowedFd};
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::remote::{Remote, Use};
use crate::update::restart;

use super::{Link, Source};

mod shared;
mod watch;

#[cfg(test)]
pub(crate) use shared::share_socket;
pub(crate) use shared::{line_done, share, unshare};
pub(crate) use watch::{input_ended, reader_gone_within, watched};

const STALE_BOOT: &str = "stale-boot";

static VIEW: OnceLock<u64> = OnceLock::new();
static STRUCTURED: OnceLock<u32> = OnceLock::new();
static BOOT: OnceLock<String> = OnceLock::new();
static REMOTE: OnceLock<Remote> = OnceLock::new();
static CHECKED: Mutex<Option<Link>> = Mutex::new(None);

pub(crate) fn use_remote(remote: Remote) {
    let _ = REMOTE.set(remote);
}

pub(crate) fn used_remote() -> Option<&'static Remote> {
    REMOTE.get()
}

pub(crate) fn use_view(view: u64) -> Result<()> {
    require("views", "has no views")?;
    if shared::active() {
        shared::set_view(view);
    } else {
        let _ = VIEW.set(view);
    }
    Ok(())
}

pub(crate) fn use_boot(boot: &str) -> Result<()> {
    require("boot", "would ignore --boot and act on whatever has that id now")?;
    if shared::active() {
        shared::set_boot(boot);
    } else {
        let _ = BOOT.set(boot.to_string());
    }
    Ok(())
}

pub(crate) fn use_machine_boot() -> Result<()> {
    require("machine-boot", "would ignore --boot with --machine and act on whatever has that id on the machine now")?;
    if shared::active() {
        shared::set_contract();
    } else {
        let _ = STRUCTURED.set(control::contract::CONTRACT);
    }
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

pub(crate) fn use_recap() -> Result<()> {
    require("recap", "has no recap")
}

pub(crate) fn use_bots() -> Result<()> {
    require("bots", "has no bots")
}

pub(crate) fn use_issues() -> Result<()> {
    require("issues", "cannot start work from an issue")
}

pub(crate) fn use_issue_names() -> Result<()> {
    require("issue-names", "cannot name the workspace an issue starts in")
}

pub(crate) fn use_flow_issue_prompt() -> Result<()> {
    require("flow-issue-prompt", "cannot start a flow on an issue with a prompt of your own")
}

pub(crate) fn use_todo_panel() -> Result<()> {
    require("todo-panel", "cannot edit, uncheck, reorder or restore TODO items")
}

pub(crate) fn use_queue_undo() -> Result<()> {
    require("queue-undo", "cannot undo queue removals or clear started entries")
}

pub(crate) fn use_queue_results() -> Result<()> {
    require("queue-results", "cannot show what a queued agent did")
}

pub(crate) fn use_queue_routines() -> Result<()> {
    require("queue-routines", "cannot repeat queue entries, run flows from the queue, or pause and resume routines")
}

pub(crate) fn use_mode_default() -> Result<()> {
    require("mode-default", "cannot queue an agent with no mode arguments")
}

pub(crate) fn use_plugin_run_view() -> Result<()> {
    require("plugin-run-view", "cannot run a plugin's action in a view")
}

pub(crate) fn use_flow_run_view() -> Result<()> {
    require("flow-run-view", "cannot start a flow in a view")
}

pub(crate) fn use_flow_close_when_merged() -> Result<()> {
    require("flow-close-when-merged", "cannot close a flow's agents once its pull request merges")
}

pub(crate) fn use_flow_free_builds() -> Result<()> {
    require("flow-free-builds", "cannot free a flow's builds once its pull request merges")
}

pub(crate) fn use_fresh_branches() -> Result<()> {
    require("fresh-branches", "cannot make a new branch from the up-to-date default branch")
}

pub(crate) fn use_build_folders() -> Result<()> {
    require("build-folders", "cannot set a project's build folders")
}

pub(crate) fn use_settings_search() -> Result<()> {
    require("settings-search", "cannot search the settings")
}

pub(crate) fn use_search_settings() -> Result<()> {
    require("search-settings", "cannot search the settings with what was said")
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

pub(crate) fn use_account_login_tab() -> Result<()> {
    require("account-login-tab", "cannot open a login tab for an account that exists, or show one in a view")
}

pub(crate) fn use_account_switch() -> Result<()> {
    require("account-switch", "cannot rename or switch accounts")
}

pub(crate) fn use_account_restart() -> Result<()> {
    require("account-restart", "cannot restart agents on another account")
}

pub(crate) fn use_machine_usage() -> Result<()> {
    require("machine-usage", "cannot read the stats and usage of other machines")
}

pub(crate) fn use_stats_cached() -> Result<()> {
    require("stats-cached", "cannot read a machine's last stats without asking it")
}

pub(crate) fn use_machine_start() -> Result<()> {
    require("machine-start", "cannot start agents, workspaces or flows on another machine")
}

pub(crate) fn use_open_into() -> Result<()> {
    require("open-code-workspace", "cannot open a project into a group")
}

pub(crate) fn use_placed_start() -> Result<()> {
    require("machine-start", "cannot answer a start as soon as its tab is open")
}

pub(crate) fn use_tunnel_hold() -> Result<()> {
    require("tunnel-hold", "cannot hold a tunnel for a program")
}

pub(crate) fn use_tunnel_reverse() -> Result<()> {
    require("tunnel-reverse", "has no reverse tunnels")
}

pub(crate) fn use_tunnel_orders() -> Result<()> {
    require("tunnel-orders", "cannot send tunnel orders to a program with no window")
}

pub(crate) fn own_connection() -> Result<()> {
    if shared::active() { Err(Error::OwnConnection) } else { Ok(()) }
}

fn require(feature: &str, lacks: &str) -> Result<()> {
    if shared::active() {
        return shared::require(feature, lacks);
    }
    let link = require_on(connect(), feature, lacks, used_remote())?;
    *CHECKED.lock() = Some(link);
    Ok(())
}

fn missing(lacks: &str, remote: Option<&Remote>) -> Error {
    let Some(remote) = remote else {
        return Error::Control(format!(
            "the running cornercase server {lacks}; update cornercase and restart the server (this one is {})",
            crate::version::CURRENT
        ));
    };
    Error::Control(format!(
        "the cornercase server on `{}` {lacks}; restart it there with `{}` (update it first with `{}` if needed)",
        remote.host(),
        remote.line("restart"),
        remote.update_line()
    ))
}

fn require_on(link: Result<Link>, feature: &str, lacks: &str, remote: Option<&Remote>) -> Result<Link> {
    let lacking = || missing(lacks, remote);
    let asked = link.and_then(|mut link| {
        let value = ask_on(&mut link, "contract", control::Command::Contract(control::Contract {}))?;
        Ok((link, value))
    });
    let (link, value) =
        asked.map_err(|e| if matches!(e, Error::Control(_) | Error::OldServer(_)) { lacking() } else { e })?;
    let descriptor: control::contract::Descriptor = answer(value)?;
    offered(&descriptor.features, feature, lacks, remote).map(|()| link)
}

fn offered(features: &[String], feature: &str, lacks: &str, remote: Option<&Remote>) -> Result<()> {
    if features.iter().any(|offered| offered == feature) { Ok(()) } else { Err(missing(lacks, remote)) }
}

pub(crate) fn close_unused_link() {
    let unused = CHECKED.lock().take();
    drop(unused);
}

pub(crate) fn connect() -> Result<Link> {
    let checked = CHECKED.lock().take();
    checked.map_or_else(|| connect_within(None), Ok)
}

fn connect_within(timeout: Option<Duration>) -> Result<Link> {
    if shared::active() {
        return Err(Error::OwnConnection);
    }
    if let Some(remote) = used_remote() {
        return Link::remote(remote, Use::Command);
    }
    let stream = super::running()?;
    stream.set_read_timeout(timeout)?;
    Link::socket(stream)
}

pub(crate) fn ask(name: &'static str, command: control::Command) -> Result<Value> {
    if shared::active() {
        return shared::ask(name, command);
    }
    ask_on(&mut connect()?, name, command)
}

pub(crate) fn ask_starting(name: &'static str, command: control::Command) -> Result<Value> {
    if shared::active() {
        return shared::ask(name, command);
    }
    let mut link = match used_remote() {
        Some(remote) => Link::remote(remote, Use::Command)?,
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

impl Link {
    pub(crate) fn ask(&mut self, name: &'static str, command: control::Command) -> Result<Value> {
        ask_on(self, name, command)
    }
}

fn ask_on(link: &mut Link, name: &'static str, command: control::Command) -> Result<Value> {
    ask_with(link, name, command, Envelope::of_the_invocation(), &mut None)
}

fn ask_with(
    link: &mut Link,
    name: &'static str,
    command: control::Command,
    envelope: Envelope,
    out: &mut Option<BorrowedFd<'_>>,
) -> Result<Value> {
    if let Err(e) = send_request(&mut link.writer, command, envelope) {
        return Err(lost(link, server_left(e)));
    }
    loop {
        if reader_left(&link.reader, out) {
            link.reader.close();
            return Err(Error::ReaderGone);
        }
        if let Some(done) = received(link, name) {
            return done;
        }
    }
}

fn received(link: &mut Link, name: &'static str) -> Option<Result<Value>> {
    match protocol::recv::<ServerMessage>(&mut link.reader) {
        Ok(Some(ServerMessage::Response(text))) => Some(answered_through(&text, link.guard.remote.as_ref())),
        Ok(Some(ServerMessage::Rejected(_))) => Some(Err(Error::OldServer(name))),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => Some(Err(Error::OldServer(name))),
        Ok(Some(ServerMessage::Frame(_) | ServerMessage::Detached)) => None,
        Ok(Some(ServerMessage::Shutdown | ServerMessage::Restart(_))) => Some(Err(Error::ServerGone)),
        Ok(Some(message)) => {
            debug_assert!(message.window_only());
            None
        }
        Ok(None) | Err(_) => Some(Err(lost(link, Error::ServerGone))),
    }
}

fn server_left(error: Error) -> Error {
    match error {
        Error::Io(e) if matches!(e.kind(), io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset) => {
            Error::ServerGone
        }
        error => error,
    }
}

pub(crate) fn request(stream: &mut impl Write, command: control::Command) -> Result<()> {
    send_request(stream, command, Envelope::of_the_invocation())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Envelope {
    view: Option<u64>,
    boot: Option<String>,
    contract: Option<u32>,
}

impl Envelope {
    fn of_the_invocation() -> Self {
        Self { view: VIEW.get().copied(), boot: BOOT.get().cloned(), contract: STRUCTURED.get().copied() }
    }

    fn for_command(self, command: &control::Command) -> Self {
        if matches!(command, control::Command::Contract(_)) { Self::default() } else { self }
    }
}

fn send_request(stream: &mut impl Write, command: control::Command, envelope: Envelope) -> Result<()> {
    let caller = std::env::var(control::PANE_ENV).ok().and_then(|id| id.parse().ok());
    let server = std::env::var(control::SERVER_ENV).ok();
    let Envelope { view, boot, contract } = envelope.for_command(&command);
    let request = serde_json::to_string(&Request { caller, server, contract, boot, view, command })
        .map_err(|e| Error::Control(e.to_string()))?;
    protocol::send(stream, &ClientMessage::Request(request))?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Heard {
    Ok(Value),
    Error(Failure),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Failure {
    Legacy(String),
    Structured { code: String, message: String },
}

pub(crate) fn answered(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(Heard::Ok(value)) => Ok(value),
        Ok(Heard::Error(Failure::Structured { code, message })) if code == STALE_BOOT => Err(Error::StaleBoot(message)),
        Ok(Heard::Error(Failure::Legacy(message) | Failure::Structured { message, .. })) => {
            Err(Error::Control(message))
        }
        Err(e) => Err(Error::Control(format!("cannot read the server's answer: {e}"))),
    }
}

pub(crate) fn hinted(text: String) -> String {
    match used_remote() {
        Some(remote) => remote.hinted(&text),
        None => text,
    }
}

fn answered_through(text: &str, remote: Option<&Remote>) -> Result<Value> {
    let Some(remote) = remote else {
        return answered(text);
    };
    match answered(text) {
        Ok(mut value) => {
            remote.hint_answer(text, &mut value);
            Ok(value)
        }
        Err(Error::Control(message)) => Err(Error::Control(remote.hinted(&message))),
        Err(Error::StaleBoot(message)) => Err(Error::StaleBoot(remote.hinted(&message))),
        Err(other) => Err(other),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamEnd {
    Left,
    Detached,
    Stopped,
    Restarted,
}

pub(crate) fn follow(
    name: &'static str,
    command: control::Command,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    follow_from(name, command, false, each).map(|_| ())
}

pub(crate) fn follow_all(
    name: &'static str,
    command: control::Command,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<()> {
    follow_from(name, command, true, each).map(|_| ())
}

pub(crate) fn follow_to_end(
    name: &'static str,
    command: control::Command,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<StreamEnd> {
    follow_from(name, command, true, each)
}

fn follow_from(
    name: &'static str,
    command: control::Command,
    first: bool,
    each: impl FnMut(Value) -> Result<bool>,
) -> Result<StreamEnd> {
    follow_link(&mut connect()?, name, command, first, each)
}

fn follow_link(
    link: &mut Link,
    name: &'static str,
    command: control::Command,
    first: bool,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<StreamEnd> {
    let answer = ask_on(link, name, command)?;
    if first && each(answer)? {
        return Ok(StreamEnd::Left);
    }
    match follow_on(&mut link.reader, watched(console::stdout()), link.guard.remote.as_ref(), each) {
        Err(Error::ServerLost) => Err(lost(link, Error::ServerLost)),
        followed => followed,
    }
}

fn follow_on(
    mut stream: impl Source,
    mut out: Option<BorrowedFd<'_>>,
    remote: Option<&Remote>,
    mut each: impl FnMut(Value) -> Result<bool>,
) -> Result<StreamEnd> {
    loop {
        if reader_left(&stream, &mut out) {
            stream.close();
            return Ok(StreamEnd::Left);
        }
        match protocol::recv::<ServerMessage>(&mut stream) {
            Ok(Some(ServerMessage::Response(text))) => {
                if each(answered_through(&text, remote)?)? {
                    return Ok(StreamEnd::Left);
                }
            }
            Ok(Some(ServerMessage::Detached)) => return Ok(StreamEnd::Detached),
            Ok(Some(ServerMessage::Shutdown)) => return Ok(StreamEnd::Stopped),
            Ok(Some(ServerMessage::Restart(_))) => return Ok(StreamEnd::Restarted),
            Ok(Some(ServerMessage::Frame(_) | ServerMessage::Rejected(_))) => {}
            Ok(Some(message)) => debug_assert!(message.window_only()),
            Ok(None) | Err(_) => return Err(Error::ServerLost),
        }
    }
}

fn reader_left(stream: &impl Source, out: &mut Option<BorrowedFd<'_>>) -> bool {
    !stream.buffered() && watch::reader_left(stream.fd(), out)
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

#[cfg(test)]
mod tests;
