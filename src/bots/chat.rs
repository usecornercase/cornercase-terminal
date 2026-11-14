use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::agents::headless::Used;
use crate::os::fs::OpenOptionsExt;

pub(crate) const NEW_CONVERSATION: &str = "new conversation";
pub(crate) const STOPPED: &str = "stopped";
pub(crate) const INTERRUPTED: &str = "interrupted by a restart";

pub(crate) fn dropped(count: usize) -> String {
    if count == 1 { String::from("1 held message dropped") } else { format!("{count} held messages dropped") }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    You,
    Bot,
    Step,
    Error,
    Notice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Line {
    pub(crate) id: u64,
    pub(crate) at: i64,
    pub(crate) kind: Kind,
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tool: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) held: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tokens: Option<Used>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) conversation: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Answer {
    pub(crate) model: Option<String>,
    pub(crate) tokens: Used,
    pub(crate) conversation: Option<String>,
}

impl Line {
    pub(crate) fn new(id: u64, at: i64, kind: Kind, text: impl Into<String>) -> Self {
        Self { id, at, kind, text: text.into(), tool: None, held: false, model: None, tokens: None, conversation: None }
    }

    pub(crate) fn you(id: u64, at: i64, text: &str, held: bool) -> Self {
        Self { held, ..Self::new(id, at, Kind::You, text) }
    }

    pub(crate) fn step(id: u64, at: i64, tool: &str, text: &str) -> Self {
        Self { tool: Some(tool.to_string()), ..Self::new(id, at, Kind::Step, text) }
    }

    pub(crate) fn answer(id: u64, at: i64, text: &str, answer: Answer) -> Self {
        Self {
            model: answer.model,
            tokens: Some(answer.tokens),
            conversation: answer.conversation,
            ..Self::new(id, at, Kind::Bot, text)
        }
    }

    pub(crate) fn failed(id: u64, at: i64, text: &str) -> Self {
        Self::new(id, at, Kind::Error, text)
    }

    pub(crate) fn notice(id: u64, at: i64, text: &str) -> Self {
        Self::new(id, at, Kind::Notice, text)
    }
}

pub(crate) fn append(path: &Path, line: &Line) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).mode(0o600).open(path)?;
    let mut bytes = serde_json::to_vec(line)?;
    bytes.push(b'\n');
    file.write_all(&bytes)
}

pub(crate) fn heal(path: &Path) -> io::Result<()> {
    let Ok(mut file) = OpenOptions::new().read(true).append(true).open(path) else { return Ok(()) };
    let length = file.metadata()?.len();
    if length == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(length - 1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    if last[0] != b'\n' {
        file.write_all(b"\n")?;
    }
    Ok(())
}

pub(crate) fn read(path: &Path) -> Vec<Line> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    String::from_utf8_lossy(&bytes).lines().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Replayed {
    pub(crate) next_id: u64,
    pub(crate) running: bool,
    pub(crate) held: usize,
}

fn dropped_in(text: &str) -> usize {
    let Some(count) =
        text.strip_suffix(" held message dropped").or_else(|| text.strip_suffix(" held messages dropped"))
    else {
        return 0;
    };
    count.parse().unwrap_or_default()
}

pub(crate) fn replay(lines: &[Line]) -> Replayed {
    let mut state = Replayed { next_id: 1, ..Replayed::default() };
    for line in lines {
        state.next_id = state.next_id.max(line.id + 1);
        match line.kind {
            Kind::You if line.held => state.held += 1,
            Kind::You => state.running = true,
            Kind::Bot | Kind::Error => {
                state.running = state.held > 0;
                state.held = 0;
            }
            Kind::Notice if line.text == STOPPED => state.running = false,
            Kind::Notice if line.text == INTERRUPTED => {
                state.running = false;
                state.held = 0;
            }
            Kind::Notice => state.held = state.held.saturating_sub(dropped_in(&line.text)),
            Kind::Step => {}
        }
    }
    if !state.running && state.held > 0 {
        state.running = true;
        state.held = 0;
    }
    state
}

pub(crate) fn restart_notices(state: Replayed, at: i64) -> Vec<Line> {
    let mut notices = Vec::new();
    let mut id = state.next_id;
    let mut add = |text: String| {
        notices.push(Line::notice(id, at, &text));
        id += 1;
    };
    if state.running {
        add(INTERRUPTED.to_string());
    }
    if state.held > 0 {
        add(dropped(state.held));
    }
    notices
}

#[cfg(test)]
mod tests;
