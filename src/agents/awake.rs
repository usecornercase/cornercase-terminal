use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::log;

pub(crate) const KEEP_FOR: Duration = Duration::from_secs(30);
const REASON: &str = "an agent is working in cornercase";
const STDERR_WAIT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tool {
    pub(crate) label: String,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
}

impl Tool {
    pub(crate) fn new(label: &str, program: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self { label: label.into(), program: program.into(), args: args.iter().map(|a| (*a).to_string()).collect() }
    }
}

pub(crate) fn tools() -> Vec<Tool> {
    if cfg!(target_os = "macos") {
        let pid = std::process::id().to_string();
        return vec![Tool::new("caffeinate", "/usr/bin/caffeinate", &["-i", "-w", &pid])];
    }
    let mut tools = Vec::new();
    if session_bus() {
        let args = ["--inhibit", "suspend", "--app-id", "cornercase", "--reason", REASON, "cat"];
        tools.push(Tool::new("gnome-session-inhibit", "gnome-session-inhibit", &args));
    }
    let why = format!("--why={REASON}");
    for what in ["--what=sleep:idle", "--what=idle"] {
        let args = [what, "--mode=block", "--who=cornercase", why.as_str(), "cat"];
        tools.push(Tool::new(&format!("systemd-inhibit {what}"), "systemd-inhibit", &args));
    }
    tools
}

fn session_bus() -> bool {
    std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some_and(|address| !address.is_empty())
        || std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| Path::new(&dir).join("bus").exists())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum State {
    Off,
    Released,
    Held(String),
    Unavailable(String),
}

impl State {
    pub(crate) fn note(&self) -> String {
        match self {
            Self::Off => "no idle sleep while an agent works".into(),
            Self::Released => "not held: no agent is working".into(),
            Self::Held(tool) => format!("held now, by {tool}"),
            Self::Unavailable(reason) => format!("not possible here: {reason}"),
        }
    }
}

struct Holder {
    tool: usize,
    child: Child,
    stdin: Option<ChildStdin>,
    stderr: Receiver<String>,
}

pub(crate) struct Awake {
    tools: Vec<Tool>,
    on: bool,
    holder: Option<Holder>,
    next: usize,
    failed: Option<String>,
    unavailable: Option<String>,
    idle_since: Option<Instant>,
}

impl Awake {
    pub(crate) fn new(tools: Vec<Tool>) -> Self {
        Self { tools, on: false, holder: None, next: 0, failed: None, unavailable: None, idle_since: None }
    }

    pub(crate) fn update(&mut self, on: bool, working: usize, now: Instant) {
        if !on {
            self.release();
            *self = Self::new(std::mem::take(&mut self.tools));
            return;
        }
        self.on = true;
        self.check();
        if working > 0 {
            self.idle_since = None;
            if self.holder.is_none() && self.unavailable.is_none() {
                self.take(working);
            }
        } else if self.holder.is_some() {
            let since = *self.idle_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= KEEP_FOR {
                self.release();
            }
        }
    }

    pub(crate) fn state(&self) -> State {
        match (&self.holder, &self.unavailable) {
            _ if !self.on => State::Off,
            (Some(holder), _) => State::Held(self.tools[holder.tool].label.clone()),
            (None, Some(reason)) => State::Unavailable(reason.clone()),
            (None, None) => State::Released,
        }
    }

    fn take(&mut self, working: usize) {
        while let Some(tool) = self.tools.get(self.next) {
            match spawn(tool) {
                Ok((child, stdin, stderr)) => {
                    log::info!("awake", "keep awake held", tool = tool.label, agents = working);
                    self.holder = Some(Holder { tool: self.next, child, stdin, stderr });
                    return;
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    self.failed = Some(format!("{} is not installed", tool.label));
                }
                Err(e) => self.failed = Some(format!("{}: {e}", tool.label)),
            }
            self.next += 1;
        }
        let reason = self.failed.take().unwrap_or_else(|| "no tool keeps this system awake".into());
        log::warning!("awake", "cannot keep awake", reason = reason);
        self.unavailable = Some(reason);
    }

    fn check(&mut self) {
        let Some(holder) = &mut self.holder else { return };
        let status = match holder.child.try_wait() {
            Ok(None) => return,
            Ok(Some(status)) => status.to_string(),
            Err(e) => e.to_string(),
        };
        let Some(mut holder) = self.holder.take() else { return };
        drop(holder.stdin.take());
        let said = holder.stderr.recv_timeout(STDERR_WAIT).unwrap_or_default();
        let last = said.lines().map(str::trim).rfind(|line| !line.is_empty()).unwrap_or(&status);
        let reason = format!("{}: {last}", self.tools[holder.tool].label);
        log::warning!("awake", "keep awake lost", reason = reason);
        self.failed = Some(reason);
        self.next = holder.tool + 1;
        self.idle_since = None;
    }

    fn release(&mut self) {
        let Some(mut holder) = self.holder.take() else { return };
        drop(holder.stdin.take());
        let _ = holder.child.kill();
        let _ = holder.child.wait();
        log::info!("awake", "keep awake released", tool = self.tools[holder.tool].label);
        self.next = holder.tool;
        self.failed = None;
        self.idle_since = None;
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        self.release();
    }
}

fn spawn(tool: &Tool) -> std::io::Result<(Child, Option<ChildStdin>, Receiver<String>)> {
    let mut child = Command::new(&tool.program)
        .args(&tool.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdin = child.stdin.take();
    let (tx, rx) = mpsc::channel();
    if let Some(mut pipe) = child.stderr.take() {
        thread::spawn(move || {
            let mut said = String::new();
            let _ = pipe.read_to_string(&mut said);
            let _ = tx.send(said);
        });
    }
    Ok((child, stdin, rx))
}

#[cfg(test)]
mod tests;
