use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::accounts::Account;
use crate::agents::Known;
use crate::agents::probe::{self, Limits, Step};
use crate::error::{Error, Result};
use crate::text::hex;
use crate::time;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Page {
    Limits,
    #[default]
    Stats,
}
impl Page {
    pub(crate) const NAMES: [&'static str; 2] = ["Limits", "Stats"];
    pub(crate) fn index(self) -> usize {
        usize::from(self == Self::Stats)
    }
    #[must_use]
    pub(crate) fn switched(self) -> Self {
        if self == Self::Stats { Self::Limits } else { Self::Stats }
    }
}

pub(crate) const TIMEOUT: Duration = Duration::from_secs(10);
const WARNING_FROM: u16 = 75;
const CRITICAL_FROM: u16 = 90;
const FINGERPRINT_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    Normal,
    Warning,
    Critical,
}

impl Severity {
    pub(crate) fn of(percent: u16) -> Self {
        if percent >= CRITICAL_FROM {
            Self::Critical
        } else if percent >= WARNING_FROM {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub(crate) fn parse(text: Option<&str>, percent: u16) -> Self {
        match text {
            Some("normal") => Self::Normal,
            Some("warning") => Self::Warning,
            _ => Self::of(percent),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Window {
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) scope: Option<String>,
    pub(crate) percent: u16,
    pub(crate) severity: Severity,
    pub(crate) resets_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Report {
    pub(crate) plan: Option<String>,
    pub(crate) limited: bool,
    pub(crate) windows: Vec<Window>,
    pub(crate) extra: Option<String>,
    pub(crate) notice: Option<&'static str>,
    pub(crate) identity: Option<String>,
}

impl Report {
    pub(crate) fn notice_for(&self, agent: Known) -> Option<&'static str> {
        self.notice.or_else(|| agent.usage().filter(|_| !self.limited).map(|spec| spec.no_limits))
    }
}

pub(crate) struct Spec {
    pub(in crate::agents) command: fn(&str) -> Command,
    pub(in crate::agents) requests: fn() -> String,
    pub(in crate::agents) answer: Answer,
    pub(crate) no_limits: &'static str,
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=999 first")]
fn whole(percent: Option<f64>) -> u16 {
    percent.unwrap_or(0.0).clamp(0.0, 999.0).round() as u16
}

#[derive(Clone, Copy)]
pub(in crate::agents) struct Reading<'a> {
    pub(in crate::agents) percent: Option<f64>,
    pub(in crate::agents) severity: Option<&'a str>,
    pub(in crate::agents) resets_at: Option<i64>,
}

pub(in crate::agents) fn window(label: String, kind: String, scope: Option<String>, reading: Reading<'_>) -> Window {
    let percent = whole(reading.percent);
    let severity = Severity::parse(reading.severity, percent);
    Window { label, kind, scope, percent, severity, resets_at: reading.resets_at }
}

pub(in crate::agents) fn fingerprint(agent: Known, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(agent.name().as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex(&hasher.finalize()[..FINGERPRINT_BYTES])
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Heard {
    pub(in crate::agents) identity: Option<String>,
}

type Answer = fn(&str, &mut Heard) -> Option<Result<Report>>;

fn run(agent: Known, command: Command, requests: &str, answer: Answer, timeout: Duration) -> Result<Report> {
    let limits = Limits { name: agent.name(), timeout, grace: Duration::ZERO };
    let mut heard = Heard::default();
    probe::run(limits, command, requests, move |line| answer(line, &mut heard).map_or(Step::Read, Step::Answer))
}

pub(crate) fn probe(agent: Known, program: &str, home: Option<&Path>, timeout: Duration) -> Result<Report> {
    let spec = agent.usage().ok_or_else(|| Error::Probe(format!("{} has no plan probe", agent.name())))?;
    run(agent, command(agent, &spec, program, home), &(spec.requests)(), spec.answer, timeout)
}

fn command(agent: Known, spec: &Spec, program: &str, home: Option<&Path>) -> Command {
    let mut command = (spec.command)(program);
    if let (Some(home), Some(account)) = (home, agent.account()) {
        for key in account.remove {
            command.env_remove(key);
        }
        command.env(account.env, home);
    }
    command
}

pub(crate) fn until(secs: i64) -> String {
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

pub(crate) fn updated(secs: u64) -> String {
    if secs < 60 {
        "updated just now".into()
    } else {
        format!("updated {} ago", time::age(i64::try_from(secs).unwrap_or(i64::MAX)))
    }
}

pub(crate) type Probe = probe::Probe<Report>;

pub(crate) type Key = (Known, Option<String>);

static UNASKED: Probe = Probe::UNASKED;

#[derive(Debug, Default)]
pub(crate) struct State {
    shown: Vec<Key>,
    probes: Vec<(Key, Probe)>,
}

impl State {
    pub(crate) fn start(&mut self, shown: Vec<(Known, Option<Account>)>) -> Vec<(Known, Option<Account>)> {
        self.shown = shown.iter().map(|(agent, account)| key(*agent, account.as_ref())).collect();
        shown.into_iter().filter(|(agent, account)| self.entry(key(*agent, account.as_ref())).start()).collect()
    }

    pub(crate) fn answered(&mut self, key: Key, result: Result<Report>, now: Instant) {
        self.entry(key).answered(result, now);
    }

    #[cfg(test)]
    pub(crate) fn probe(&self, agent: Known) -> &Probe {
        self.probe_of(agent, None)
    }

    pub(crate) fn probe_of(&self, agent: Known, account: Option<&str>) -> &Probe {
        self.probes
            .iter()
            .find(|((kind, name), _)| *kind == agent && name.as_deref() == account)
            .map_or(&UNASKED, |(_, probe)| probe)
    }

    pub(crate) fn shown(&self) -> impl Iterator<Item = (Known, Option<&str>, &Probe)> {
        self.shown
            .iter()
            .map(|(agent, account)| (*agent, account.as_deref(), self.probe_of(*agent, account.as_deref())))
    }

    fn entry(&mut self, key: Key) -> &mut Probe {
        let at = self.probes.iter().position(|(k, _)| *k == key).unwrap_or_else(|| {
            self.probes.push((key, Probe::default()));
            self.probes.len() - 1
        });
        &mut self.probes[at].1
    }
}

pub(crate) fn heading(title: &str, account: Option<&str>, active: bool) -> String {
    let mark = if active { "● " } else { "" };
    account.map_or_else(|| format!("{mark}{title}"), |account| format!("{mark}{title} · {account}"))
}

pub(crate) fn key(agent: Known, account: Option<&Account>) -> Key {
    (agent, account.map(|account| account.name.clone()))
}

#[cfg(test)]
mod tests;
