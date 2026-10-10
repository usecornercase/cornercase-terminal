use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::log;

pub const KEEP_FOR: Duration = Duration::from_secs(30);
const REASON: &str = "an agent is working in cornercase";
const STDERR_WAIT: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub label: String,
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl Tool {
    pub fn new(label: &str, program: impl Into<PathBuf>, args: &[&str]) -> Self {
        Self { label: label.into(), program: program.into(), args: args.iter().map(|a| (*a).to_string()).collect() }
    }
}

pub fn tools() -> Vec<Tool> {
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
pub enum State {
    Off,
    Released,
    Held(String),
    Unavailable(String),
}

impl State {
    pub fn note(&self) -> String {
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

pub struct Awake {
    tools: Vec<Tool>,
    on: bool,
    holder: Option<Holder>,
    next: usize,
    failed: Option<String>,
    unavailable: Option<String>,
    idle_since: Option<Instant>,
}

impl Awake {
    pub fn new(tools: Vec<Tool>) -> Self {
        Self { tools, on: false, holder: None, next: 0, failed: None, unavailable: None, idle_since: None }
    }

    pub fn update(&mut self, on: bool, working: usize, now: Instant) {
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

    pub fn state(&self) -> State {
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
mod tests {
    use super::*;
    use crate::test_util::{TempDir, wait_until, write_executable};

    const HOLDS: &str = "#!/bin/sh\necho \"$@\" >> \"$0.args\"\necho $$ > \"$0.pid\"\nexec cat\n";
    const REFUSES: &str =
        "#!/bin/sh\necho \"$@\" >> \"$0.args\"\necho 'Failed to inhibit: Access denied' >&2\nexit 1\n";

    struct Fakes {
        dir: TempDir,
    }

    impl Fakes {
        fn new() -> Self {
            Self { dir: TempDir::new() }
        }

        fn tool(&self, name: &str, script: &str) -> Tool {
            let path = self.dir.path().join(name);
            write_executable(&path, script);
            Tool::new(name, path, &["--why=working", "cat"])
        }

        fn calls(&self, name: &str) -> usize {
            std::fs::read_to_string(self.dir.path().join(format!("{name}.args"))).map_or(0, |text| text.lines().count())
        }

        fn pid(&self, name: &str) -> i32 {
            let path = self.dir.path().join(format!("{name}.pid"));
            wait_until("the fake writes its pid", || std::fs::read_to_string(&path).is_ok_and(|t| t.ends_with('\n')));
            std::fs::read_to_string(path).expect("the pid").trim().parse().expect("a pid")
        }
    }

    fn gone(pid: i32) -> bool {
        !crate::process::alive(pid)
    }

    fn settled(awake: &mut Awake, working: usize, now: Instant) {
        wait_until("the refusal is seen", || {
            awake.update(true, working, now);
            !matches!(awake.state(), State::Held(ref tool) if tool == "refuses")
        });
    }

    #[test]
    fn holds_while_an_agent_works_with_the_tools_arguments() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);

        awake.update(true, 2, Instant::now());
        let pid = fakes.pid("holds");
        let args = std::fs::read_to_string(fakes.dir.path().join("holds.args")).expect("the arguments");

        assert_eq!((awake.state(), args.as_str()), (State::Held("holds".into()), "--why=working cat\n"));
        assert!(!gone(pid));
    }

    #[test]
    fn nothing_is_held_while_no_agent_works_or_the_setting_is_off() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);

        awake.update(true, 0, Instant::now());
        let idle = awake.state();
        awake.update(false, 3, Instant::now());

        assert_eq!((idle, awake.state(), fakes.calls("holds")), (State::Released, State::Off, 0));
    }

    #[test]
    fn lets_go_once_no_agent_has_worked_for_a_while() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
        let start = Instant::now();
        awake.update(true, 1, start);
        let pid = fakes.pid("holds");

        awake.update(true, 0, start + Duration::from_secs(1));
        awake.update(true, 0, start + KEEP_FOR);
        let kept = awake.state();
        awake.update(true, 0, start + Duration::from_secs(1) + KEEP_FOR);

        assert_eq!((kept, awake.state()), (State::Held("holds".into()), State::Released));
        wait_until("the holder ends", || gone(pid));
    }

    #[test]
    fn work_again_within_the_grace_keeps_the_same_holder() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
        let start = Instant::now();
        awake.update(true, 1, start);

        awake.update(true, 0, start + Duration::from_secs(1));
        awake.update(true, 1, start + Duration::from_secs(20));
        awake.update(true, 0, start + Duration::from_secs(40));
        awake.update(true, 0, start + Duration::from_secs(60));
        fakes.pid("holds");

        assert_eq!((awake.state(), fakes.calls("holds")), (State::Held("holds".into()), 1));
    }

    #[test]
    fn turning_it_off_lets_go_at_once() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
        awake.update(true, 1, Instant::now());
        let pid = fakes.pid("holds");

        awake.update(false, 1, Instant::now());

        assert_eq!(awake.state(), State::Off);
        wait_until("the holder ends", || gone(pid));
    }

    #[test]
    fn a_refused_lock_falls_back_to_the_next_tool() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("refuses", REFUSES), fakes.tool("holds", HOLDS)]);

        settled(&mut awake, 1, Instant::now());

        assert_eq!((awake.state(), fakes.calls("refuses")), (State::Held("holds".into()), 1));
    }

    #[test]
    fn says_why_when_every_tool_refuses_and_stops_trying() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("refuses", REFUSES)]);

        settled(&mut awake, 1, Instant::now());
        awake.update(true, 1, Instant::now());

        let reason = "refuses: Failed to inhibit: Access denied".to_string();
        assert_eq!((awake.state(), fakes.calls("refuses")), (State::Unavailable(reason), 1));
    }

    #[test]
    fn a_missing_tool_is_named() {
        let mut awake = Awake::new(vec![Tool::new("systemd-inhibit", "/nonexistent/systemd-inhibit", &[])]);

        awake.update(true, 1, Instant::now());

        assert_eq!(awake.state(), State::Unavailable("systemd-inhibit is not installed".into()));
    }

    #[test]
    fn the_holder_ends_with_the_end_of_its_input_as_when_the_server_dies() {
        let fakes = Fakes::new();
        let mut awake = Awake::new(vec![fakes.tool("holds", HOLDS)]);
        awake.update(true, 1, Instant::now());
        let pid = fakes.pid("holds");

        drop(awake.holder.as_mut().and_then(|h| h.stdin.take()));

        wait_until("the holder ends", || {
            awake.update(true, 1, Instant::now());
            !matches!(awake.state(), State::Held(_))
        });
        assert!(gone(pid));
    }
}
