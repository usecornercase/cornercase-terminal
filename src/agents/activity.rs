use std::ffi::OsString;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Deserialize;

pub const CLAUDE_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
pub const CLAUDE_SESSION_ENV: [&str; 10] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];
const SPINNER: [char; 4] = ['◐', '◓', '◑', '◒'];
const BRAILLE: RangeInclusive<char> = '\u{2800}'..='\u{28ff}';
const IDLE: char = '✳';
const ACTION_REQUIRED: [&str; 2] = ["[ ! ] Action Required", "[ . ] Action Required"];
const NOTIFY_AFTER: Duration = Duration::from_secs(1);
pub const SHELL: &str = "shell";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Working,
    Shell,
    Waiting,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Idle,
    Working,
    Done,
    Waiting,
}

impl Status {
    pub fn needs_you(self) -> bool {
        matches!(self, Self::Done | Self::Waiting)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Done => "done",
            Self::Waiting => "waiting",
        }
    }
}

pub fn attention(statuses: impl IntoIterator<Item = Option<Status>>) -> Option<Status> {
    statuses.into_iter().flatten().filter(|s| s.needs_you()).max()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pane {
    agent: Option<String>,
    activity: Option<Activity>,
    unseen: bool,
    since: Option<Instant>,
    notified: bool,
    worked: Option<Instant>,
    moved: Option<Instant>,
}

impl Pane {
    pub fn follow(&mut self, agent: Option<&str>) {
        if self.agent.as_deref() != agent {
            *self = Self { agent: agent.map(str::to_string), ..Self::default() };
        }
    }

    pub fn update(&mut self, activity: Option<Activity>, seen: bool, now: Instant) -> Option<Status> {
        let before = self.status();
        let finished = matches!(self.activity, Some(Activity::Working | Activity::Shell | Activity::Waiting));
        if activity == Some(Activity::Working) {
            self.worked = Some(now);
        }
        if activity.is_some() && self.activity.is_some() && activity != self.activity {
            self.moved = Some(now);
        }
        self.unseen = activity == Some(Activity::Idle) && !seen && (self.unseen || finished);
        self.activity = activity;
        let status = self.status();
        if status != before {
            self.since = Some(now);
            self.notified = false;
        }
        self.notified |= seen;
        let settled = self.since.is_some_and(|since| now.duration_since(since) >= NOTIFY_AFTER);
        if !settled || self.notified || !status.is_some_and(Status::needs_you) {
            return None;
        }
        self.notified = true;
        status
    }

    pub fn see(&mut self) {
        self.unseen = false;
        self.notified = true;
    }

    pub fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }

    pub fn reacted(&self, since: Instant) -> bool {
        [self.worked, self.moved].into_iter().flatten().any(|at| at >= since)
    }

    pub fn background_shell(&self) -> bool {
        self.activity == Some(Activity::Shell)
    }

    pub fn state(&self) -> Option<&'static str> {
        if self.background_shell() { Some(SHELL) } else { self.status().map(Status::name) }
    }

    pub fn status(&self) -> Option<Status> {
        Some(match self.activity? {
            Activity::Working | Activity::Shell => Status::Working,
            Activity::Waiting => Status::Waiting,
            Activity::Idle if self.unseen => Status::Done,
            Activity::Idle => Status::Idle,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pid: Option<i64>,
    status: Option<String>,
    #[serde(rename = "sessionId")]
    pub id: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl Session {
    pub fn read(dir: Option<&Path>, pid: i32) -> Option<Self> {
        let text = std::fs::read_to_string(dir?.join("sessions").join(format!("{pid}.json"))).ok()?;
        Self::parse(&text, pid)
    }

    pub fn parse(text: &str, pid: i32) -> Option<Self> {
        let session: Self = serde_json::from_str(text).ok()?;
        session.pid.is_none_or(|p| p == i64::from(pid)).then_some(session)
    }

    fn activity(&self) -> Option<Activity> {
        match self.status.as_deref()? {
            "busy" => Some(Activity::Working),
            "shell" => Some(Activity::Shell),
            "waiting" => Some(Activity::Waiting),
            "idle" => Some(Activity::Idle),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct Claude {
    pub pid: i32,
    pub args: Vec<String>,
    pub session: Option<Session>,
}

impl Claude {
    pub fn activity(&self, title: &str) -> Activity {
        self.session.as_ref().and_then(Session::activity).or_else(|| from_title(title)).unwrap_or(Activity::Idle)
    }
}

pub fn claude_dir(home: Option<&Path>) -> Option<PathBuf> {
    claude_dir_from(std::env::var_os(CLAUDE_DIR_ENV), home)
}

fn claude_dir_from(env: Option<OsString>, home: Option<&Path>) -> Option<PathBuf> {
    match env {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => home.map(|home| home.join(".claude")),
    }
}

pub fn codex(title: &str, turn: bool) -> Activity {
    if ACTION_REQUIRED.iter().any(|prefix| title.starts_with(prefix)) {
        Activity::Waiting
    } else if turn || glyph(title).is_some_and(|g| BRAILLE.contains(&g)) {
        Activity::Working
    } else {
        Activity::Idle
    }
}

pub fn gemini(title: &str) -> Option<Activity> {
    match glyph(title.trim())? {
        '✋' => Some(Activity::Waiting),
        '✦' | '⏲' => Some(Activity::Working),
        '◇' => Some(Activity::Idle),
        _ => None,
    }
}

pub fn opencode(turn: bool) -> Activity {
    if turn { Activity::Working } else { Activity::Idle }
}

fn glyph(title: &str) -> Option<char> {
    let mut chars = title.chars();
    let glyph = chars.next()?;
    (chars.next() == Some(' ')).then_some(glyph)
}

fn from_title(title: &str) -> Option<Activity> {
    let glyph = glyph(title)?;
    if SPINNER.contains(&glyph) || BRAILLE.contains(&glyph) {
        Some(Activity::Working)
    } else if glyph == IDLE {
        Some(Activity::Idle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
