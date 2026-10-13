mod claude;
mod codex;

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::agents;
use crate::error::{Error, Result};
use crate::time;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Page {
    Limits,
    #[default]
    Stats,
}
impl Page {
    pub const NAMES: [&'static str; 2] = ["Limits", "Stats"];
    pub fn index(self) -> usize {
        usize::from(self == Self::Stats)
    }
    #[must_use]
    pub fn switched(self) -> Self {
        if self == Self::Stats { Self::Limits } else { Self::Stats }
    }
}

pub const TIMEOUT: Duration = Duration::from_secs(10);
const WARNING_FROM: u16 = 75;
const CRITICAL_FROM: u16 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Normal,
    Warning,
    Critical,
}

impl Severity {
    pub fn of(percent: u16) -> Self {
        if percent >= CRITICAL_FROM {
            Self::Critical
        } else if percent >= WARNING_FROM {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    fn parse(text: Option<&str>, percent: u16) -> Self {
        match text {
            Some("normal") => Self::Normal,
            Some("warning") => Self::Warning,
            _ => Self::of(percent),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub label: String,
    pub percent: u16,
    pub severity: Severity,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub plan: Option<String>,
    pub limited: bool,
    pub windows: Vec<Window>,
    pub extra: Option<String>,
    pub notice: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub const ALL: [Self; 2] = [Self::Claude, Self::Codex];

    pub fn kind(self) -> &'static str {
        match self {
            Self::Claude => agents::CLAUDE,
            Self::Codex => agents::CODEX,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    pub fn no_limits(self) -> &'static str {
        match self {
            Self::Claude => "no plan limits for this account (API key, Bedrock or Vertex)",
            Self::Codex => "no plan limits for this account",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Claude => 0,
            Self::Codex => 1,
        }
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=999 first")]
fn whole(percent: Option<f64>) -> u16 {
    percent.unwrap_or(0.0).clamp(0.0, 999.0).round() as u16
}

fn window(label: String, percent: Option<f64>, severity: Option<&str>, resets_at: Option<i64>) -> Window {
    let percent = whole(percent);
    Window { label, percent, severity: Severity::parse(severity, percent), resets_at }
}

type Answer = fn(&str) -> Option<Result<Report>>;

fn read_answer(reader: impl BufRead, answer: Answer, name: &str) -> Result<Report> {
    for line in reader.lines() {
        if let Some(result) = answer(&line?) {
            return result;
        }
    }
    Err(Error::Usage(format!("{name} exited without answering")))
}

fn await_answer(
    written: Option<std::io::Result<()>>,
    stdout: Option<impl Read + Send + 'static>,
    answer: Answer,
    name: &'static str,
    timeout: Duration,
) -> Result<Report> {
    let (tx, rx) = mpsc::channel();
    if let (Some(Ok(())), Some(stdout)) = (written, stdout) {
        std::thread::spawn(move || {
            let _ = tx.send(read_answer(BufReader::new(stdout), answer, name));
        });
    } else {
        drop(tx);
    }
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Usage(format!("{name} did not answer in time"))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(Error::Usage(format!("{name} exited without answering"))),
    }
}

fn run(agent: Agent, mut command: Command, requests: &str, answer: Answer, timeout: Duration) -> Result<Report> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Usage(format!("could not run {program}: {e}")))?;
    let mut stdin = child.stdin.take();
    let written = stdin.as_mut().map(|s| s.write_all(requests.as_bytes()).and_then(|()| s.flush()));
    let answer = await_answer(written, child.stdout.take(), answer, agent.kind(), timeout);
    let _ = child.kill();
    let _ = child.wait();
    drop(stdin);
    answer
}

pub fn probe(agent: Agent, program: &str, timeout: Duration) -> Result<Report> {
    match agent {
        Agent::Claude => run(agent, claude::command(program), claude::REQUESTS, claude::answer, timeout),
        Agent::Codex => run(agent, codex::command(program), &codex::requests(), codex::answer, timeout),
    }
}

pub fn until(secs: i64) -> String {
    let minutes = (secs.max(0) + 59) / 60;
    let (days, hours, minutes) = (minutes / 1_440, minutes / 60 % 24, minutes % 60);
    if days > 0 {
        format!("resets in {days}d {hours}h")
    } else if hours > 0 {
        format!("resets in {hours}h {minutes}m")
    } else {
        format!("resets in {minutes}m")
    }
}

pub fn updated(secs: u64) -> String {
    if secs < 60 {
        "updated just now".into()
    } else {
        format!("updated {} ago", time::age(i64::try_from(secs).unwrap_or(i64::MAX)))
    }
}

#[derive(Debug, Default)]
pub struct Probe {
    last: Option<(Report, Instant)>,
    loading: bool,
    error: Option<String>,
}

impl Probe {
    fn start(&mut self) -> bool {
        if self.loading {
            return false;
        }
        self.loading = true;
        self.error = None;
        true
    }

    fn answered(&mut self, result: Result<Report>, now: Instant) {
        self.loading = false;
        match result {
            Ok(report) => self.last = Some((report, now)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    pub fn last(&self) -> Option<&(Report, Instant)> {
        self.last.as_ref()
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Debug, Default)]
pub struct State {
    shown: Vec<Agent>,
    probes: [Probe; 2],
}

impl State {
    pub fn start(&mut self, shown: Vec<Agent>) -> Vec<Agent> {
        self.shown = shown;
        self.shown.iter().copied().filter(|agent| self.probes[agent.index()].start()).collect()
    }

    pub fn answered(&mut self, agent: Agent, result: Result<Report>, now: Instant) {
        self.probes[agent.index()].answered(result, now);
    }

    pub fn shown(&self) -> impl Iterator<Item = (Agent, &Probe)> {
        self.shown.iter().map(|&agent| (agent, &self.probes[agent.index()]))
    }
}

#[cfg(test)]
pub(crate) use claude::tests as claude_tests;
#[cfg(test)]
pub(crate) use codex::tests as codex_tests;

#[cfg(test)]
mod tests;
