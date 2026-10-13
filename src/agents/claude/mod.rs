use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::agents::activity::{Activity, BRAILLE, glyph};

pub(in crate::agents) const NAME: &str = "claude";
pub(in crate::agents) const TITLE: &str = "Claude Code";
pub(in crate::agents) const MODES: crate::agents::ModeTable = &[
    ("accept edits", &["--permission-mode", "acceptEdits"]),
    ("auto", &["--permission-mode", "auto"]),
    ("plan", &["--permission-mode", "plan"]),
    ("skip permissions (dangerous)", &["--dangerously-skip-permissions"]),
];
pub(in crate::agents) const HOLDS_PROMPTS: bool = false;
pub(in crate::agents) const RUNS_AS: bool = false;
pub(in crate::agents) const LOCAL_COMMAND: bool = false;
pub(in crate::agents) const USAGE: Option<crate::agents::usage::Spec> = Some(crate::agents::usage::Spec {
    command: usage::command,
    requests: usage::requests,
    answer: usage::answer,
    no_limits: "no plan limits for this account (API key, Bedrock or Vertex)",
});
pub(in crate::agents) const UNFOUND: &str = "Claude Code has not said yet which conversation it is in";

pub(in crate::agents) fn normalized(args: &[String]) -> Vec<String> {
    args.to_vec()
}

#[expect(clippy::unnecessary_wraps, reason = "Known dispatch shares an optional resume signature across agents")]
pub(in crate::agents) fn resume(args: Vec<String>, conversation: String) -> Option<Vec<String>> {
    Some([args, vec!["--resume".into(), conversation]].concat())
}

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
const IDLE: char = '✳';
#[derive(Debug, Deserialize)]
pub(crate) struct Session {
    pid: Option<i64>,
    status: Option<String>,
    #[serde(rename = "sessionId")]
    pub(crate) id: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
}

impl Session {
    pub(crate) fn read(dir: Option<&Path>, pid: i32) -> Option<Self> {
        let text = std::fs::read_to_string(dir?.join("sessions").join(format!("{pid}.json"))).ok()?;
        Self::parse(&text, pid)
    }

    pub(crate) fn parse(text: &str, pid: i32) -> Option<Self> {
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
pub(crate) struct Claude {
    pub(crate) pid: i32,
    pub(crate) args: Vec<String>,
    pub(crate) session: Option<Session>,
}

impl Claude {
    pub(crate) fn activity(&self, title: &str) -> Activity {
        self.session.as_ref().and_then(Session::activity).or_else(|| from_title(title)).unwrap_or(Activity::Idle)
    }
}

pub(crate) fn claude_dir(home: Option<&Path>) -> Option<PathBuf> {
    claude_dir_from(std::env::var_os(CLAUDE_DIR_ENV), home)
}

fn claude_dir_from(env: Option<OsString>, home: Option<&Path>) -> Option<PathBuf> {
    match env {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => home.map(|home| home.join(".claude")),
    }
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

pub(in crate::agents) mod message;
pub(crate) mod screen;
pub(in crate::agents) mod transcript;
pub(crate) mod usage;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
pub(in crate::agents) mod tests;
