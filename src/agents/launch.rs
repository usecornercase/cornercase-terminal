use std::time::{Duration, Instant};

use super::claude;
use crate::term::bracketed;
use crate::text::one_line;

const SHELL_QUIET: Duration = Duration::from_millis(300);
const SHELL_LATEST: Duration = Duration::from_secs(5);
const AGENT_QUIET: Duration = Duration::from_secs(1);
const AGENT_GONE: Duration = Duration::from_secs(3);
const AGENT_LATEST: Duration = Duration::from_secs(30);
const SUBMIT_QUIET: Duration = Duration::from_millis(300);
const SUBMIT_LATEST: Duration = Duration::from_secs(3);
const MAX_TRUSTS: u8 = 4;
const MARKERS: [&str; 5] = ["❯", "›", ">", "▸", "→"];
const MENU_REACH: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Spec {
    pub(crate) command: String,
    pub(crate) prompt: Option<String>,
    pub(crate) submit: bool,
    pub(crate) trusted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Parked {
    Trust,
    Consent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Shell,
    Agent,
    Submit,
}

#[derive(Debug)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own fact about where the launch stands")]
pub(crate) struct Launch {
    pub(crate) term: u64,
    pub(crate) key: Option<u64>,
    pub(crate) after: Vec<u64>,
    pub(crate) told: bool,
    pub(crate) trust: Option<Trust>,
    pub(crate) resume: bool,
    spec: Spec,
    agent: bool,
    stage: Stage,
    since: Instant,
    output: Option<Instant>,
    trusts: u8,
    trusted_screen: Option<String>,
    echo: String,
    undrawn: bool,
    parked: Option<Parked>,
}

pub(crate) struct Seen<'a> {
    pub(crate) shell_in_foreground: bool,
    pub(crate) bracketed_paste: bool,
    pub(crate) application_cursor: bool,
    pub(crate) screen: &'a mut dyn FnMut() -> String,
    pub(crate) trust_prompt: &'a dyn Fn(&str) -> bool,
    pub(crate) trust: Trust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Trust {
    LeftToYou,
    Accepted,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Wait,
    Write(Vec<u8>),
    Done(Vec<u8>),
    Abandon,
}

impl Launch {
    pub(crate) fn new(term: u64, spec: Spec, now: Instant) -> Self {
        Self {
            term,
            key: None,
            after: Vec::new(),
            told: false,
            trust: None,
            resume: false,
            spec,
            agent: true,
            stage: Stage::Shell,
            since: now,
            output: None,
            trusts: 0,
            trusted_screen: None,
            echo: String::new(),
            undrawn: false,
            parked: None,
        }
    }

    pub(crate) fn wait_from(&mut self, now: Instant) {
        self.since = now;
        self.output = Some(now);
    }

    pub(crate) fn command(term: u64, command: String, now: Instant) -> Self {
        Self { agent: false, ..Self::new(term, Spec { command, prompt: None, submit: false, trusted: false }, now) }
    }

    pub(crate) fn resume(term: u64, command: String, trusted: bool, now: Instant) -> Self {
        let spec = Spec { command, prompt: None, submit: false, trusted };
        Self { trust: Some(Trust::LeftToYou), resume: true, ..Self::new(term, spec, now) }
    }

    pub(crate) fn enter(term: u64, now: Instant) -> Self {
        Self { stage: Stage::Submit, ..Self::command(term, String::new(), now) }
    }

    pub(crate) fn output(&mut self, now: Instant) {
        if self.parked.is_some() {
            self.since = now;
        }
        self.output = Some(now);
        self.undrawn = false;
        self.parked = None;
    }

    pub(crate) fn stage(&self) -> &'static str {
        match self.stage {
            Stage::Shell => "shell",
            Stage::Agent => match self.parked {
                Some(Parked::Consent) => "consent",
                Some(Parked::Trust) => "trust prompt",
                None => "agent",
            },
            Stage::Submit => "submit",
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match (self.agent, self.spec.command.is_empty()) {
            (true, _) => "agent",
            (false, false) => "command",
            (false, true) => "enter",
        }
    }

    pub(crate) fn waits_for_you(&self) -> bool {
        self.parked.is_some()
    }

    pub(crate) fn typed(&self) -> bool {
        self.stage != Stage::Shell
    }

    pub(crate) fn submits(&self) -> bool {
        self.stage == Stage::Submit
    }

    fn quiet(&self, now: Instant) -> Duration {
        now.duration_since(self.output.unwrap_or(self.since))
    }

    fn next(&mut self, stage: Stage, now: Instant) {
        self.stage = stage;
        self.since = now;
        self.output = None;
        self.undrawn = false;
    }

    pub(crate) fn step(&mut self, now: Instant, seen: &mut Seen) -> Step {
        match self.stage {
            Stage::Shell => {
                let ready = self.output.is_some_and(|at| now.duration_since(at) >= SHELL_QUIET)
                    || now.duration_since(self.since) >= SHELL_LATEST;
                if !ready {
                    return Step::Wait;
                }
                let typed = format!("{}\r", self.spec.command).into_bytes();
                if !self.agent {
                    return Step::Done(typed);
                }
                self.echo = compact(&(seen.screen)()) + &compact(&self.spec.command);
                self.next(Stage::Agent, now);
                Step::Write(typed)
            }
            Stage::Agent if seen.shell_in_foreground => {
                if now.duration_since(self.since) >= AGENT_GONE && self.quiet(now) >= AGENT_GONE {
                    Step::Abandon
                } else {
                    Step::Wait
                }
            }
            Stage::Agent if self.parked.is_some() => Step::Wait,
            Stage::Agent => {
                let latest = now.duration_since(self.since) >= AGENT_LATEST;
                if (self.quiet(now) < AGENT_QUIET || self.undrawn) && !latest {
                    return Step::Wait;
                }
                let screen = (seen.screen)();
                if !latest && only_echo(&screen, &self.echo) {
                    self.undrawn = true;
                    return Step::Wait;
                }
                if claude::screen::bypass_consent(&screen) {
                    self.next(Stage::Agent, now);
                    self.parked = Some(Parked::Consent);
                    return Step::Wait;
                }
                if (seen.trust_prompt)(&screen) {
                    if seen.trust == Trust::LeftToYou && !self.spec.trusted {
                        self.next(Stage::Agent, now);
                        self.parked = Some(Parked::Trust);
                        return Step::Wait;
                    }
                    if self.trusts < MAX_TRUSTS && self.trusted_screen.as_ref() != Some(&screen) {
                        self.trusts += 1;
                        let keys = answer_keys(&screen, seen.application_cursor);
                        self.trusted_screen = Some(screen);
                        self.next(Stage::Agent, now);
                        return Step::Write(keys);
                    }
                }
                let Some(prompt) = &self.spec.prompt else { return Step::Done(Vec::new()) };
                let text = if self.spec.submit && seen.bracketed_paste { prompt.clone() } else { one_line(prompt) };
                let bytes = if seen.bracketed_paste { bracketed(&text) } else { text };
                if self.spec.submit {
                    self.next(Stage::Submit, now);
                    Step::Write(bytes.into_bytes())
                } else {
                    Step::Done(bytes.into_bytes())
                }
            }
            Stage::Submit => {
                if self.quiet(now) >= SUBMIT_QUIET || now.duration_since(self.since) >= SUBMIT_LATEST {
                    Step::Done(b"\r".to_vec())
                } else {
                    Step::Wait
                }
            }
        }
    }
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn only_echo(screen: &str, echo: &str) -> bool {
    let mut echo = echo.chars();
    screen.chars().filter(|c| !c.is_whitespace()).all(|c| echo.next() == Some(c))
}

fn option_text(line: &str) -> &str {
    line.trim_start_matches(|c: char| c.is_whitespace() || "│┃║|".contains(c))
}

pub(crate) fn answer_keys(screen: &str, application_cursor: bool) -> Vec<u8> {
    let lines: Vec<&str> = screen.lines().map(option_text).collect();
    let Some(marked) = lines.iter().position(|l| MARKERS.iter().any(|m| l.starts_with(m))) else {
        return b"\r".to_vec();
    };
    let near = marked.saturating_sub(MENU_REACH)..(marked + MENU_REACH + 1).min(lines.len());
    let yes = near
        .filter(|&i| lines[i].to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|word| word == "yes"))
        .min_by_key(|&i| i.abs_diff(marked));
    match yes {
        Some(target) if target != marked => {
            let key = match (target > marked, application_cursor) {
                (true, false) => "\x1b[B",
                (true, true) => "\x1bOB",
                (false, false) => "\x1b[A",
                (false, true) => "\x1bOA",
            };
            key.repeat(target.abs_diff(marked)).into_bytes()
        }
        _ => b"\r".to_vec(),
    }
}

#[cfg(test)]
mod tests;
