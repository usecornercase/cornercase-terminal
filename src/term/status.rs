mod stream;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use self::stream::Stream;
use crate::agents::activity::Activity;

pub(super) const REPLY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";
const MAX_RECORDS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Idle,
    Working,
    Done,
    Error,
    Blocked,
}

impl State {
    fn of(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "working" => Some(Self::Working),
            "done" => Some(Self::Done),
            "error" => Some(Self::Error),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }

    fn activity(self) -> Activity {
        match self {
            Self::Idle => Activity::Ready,
            Self::Working => Activity::Working,
            Self::Done => Activity::Done,
            Self::Error => Activity::Failed,
            Self::Blocked => Activity::Waiting,
        }
    }
}

struct Record {
    id: String,
    state: State,
    process: Option<i32>,
}

#[derive(Default)]
pub(super) struct Status {
    stream: Stream,
    records: Vec<Record>,
    process: Option<i32>,
}

impl Status {
    pub(super) fn advance(&mut self, byte: u8) -> bool {
        let Some(command) = self.stream.advance(byte) else { return false };
        match command {
            stream::Command::Reset => self.records.clear(),
            stream::Command::Osc(bytes) => {
                let Ok(text) = std::str::from_utf8(bytes) else { return false };
                if text.split(';').next() == Some("133") && text.split(';').nth(1) == Some("A") {
                    self.records.retain(|record| matches!(record.state, State::Done | State::Error));
                } else if let Some(body) = text.strip_prefix("7501;") {
                    if body == "?" {
                        return true;
                    }
                    if let Some(report) = Report::parse(body) {
                        apply(&mut self.records, self.process, &report);
                    }
                }
            }
        }
        false
    }

    pub(super) fn process(&mut self, pid: Option<i32>) {
        if self.process != pid {
            self.records.retain(|record| matches!(record.state, State::Done | State::Error));
            self.process = pid;
        }
    }

    pub(super) fn activity(&self) -> Option<Activity> {
        self.records
            .iter()
            .filter(|record| record.process == self.process)
            .map(|record| record.state)
            .max()
            .map(State::activity)
    }

    pub(super) fn finished(&self, pid: i32) -> Option<Activity> {
        self.records
            .iter()
            .filter(|record| record.process == Some(pid) && matches!(record.state, State::Done | State::Error))
            .map(|record| record.state)
            .max()
            .map(State::activity)
    }

    pub(super) fn acknowledge(&mut self, pid: i32) {
        self.records.retain(|record| record.process != Some(pid) || record.state != State::Done);
    }
}

struct Report<'a> {
    id: Option<&'a str>,
    state: &'a str,
}

impl<'a> Report<'a> {
    fn parse(body: &'a str) -> Option<Self> {
        let mut report = Self { id: None, state: "" };
        for pair in body.split(':') {
            let Some((key, value)) = pair.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            if key.len() > 16 {
                return None;
            }
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_lowercase()) || !value.bytes().all(value_byte) {
                continue;
            }
            match key {
                "state" => report.state = value,
                "id" if value.len() <= 128 => report.id = Some(value),
                "app" if value.len() <= 32 => {}
                "msg" => validate_text(value, 2732, 2048)?,
                "title" => validate_text(value, 256, 192)?,
                "id" | "app" => return None,
                _ => {}
            }
        }
        let valid_id = report.id.is_none_or(|id| id.split('/').count() <= 8 && id.split('/').all(segment));
        (valid_id && (report.state == "clear" || State::of(report.state).is_some())).then_some(report)
    }
}

fn value_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"_.,+/=-".contains(&byte)
}

fn segment(text: &str) -> bool {
    !text.is_empty() && text.len() <= 32 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b))
}

fn validate_text(value: &str, encoded: usize, decoded: usize) -> Option<()> {
    if value.len() > encoded {
        return None;
    }
    let padded = format!("{value}{}", "=".repeat((4 - value.len() % 4) % 4));
    let bytes = STANDARD.decode(padded).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    (bytes.len() <= decoded && !text.chars().any(char::is_control)).then_some(())
}

fn apply(records: &mut Vec<Record>, process: Option<i32>, report: &Report<'_>) {
    let id = report.id.unwrap_or_default();
    if report.state == "clear" {
        records.retain(|record| {
            !id.is_empty()
                && record.id != id
                && !record.id.strip_prefix(id).is_some_and(|suffix| suffix.starts_with('/'))
        });
        return;
    }
    let Some(state) = State::of(report.state) else { return };
    records.retain(|record| record.id != id);
    if records.len() == MAX_RECORDS {
        records.remove(0);
    }
    records.push(Record { id: id.to_string(), state, process });
}

#[cfg(test)]
mod tests;
