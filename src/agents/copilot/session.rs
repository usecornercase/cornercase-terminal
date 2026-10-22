use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::events::{self, Fold};
use crate::agents::activity::Activity;
use crate::agents::context::counting::Record;
use crate::agents::context::jsonl::{self, Tail};
use crate::agents::context::{Compaction, Context, discovery, message};
use crate::process;

const BACK_DEFAULT: u64 = 64 * 1024 * 1024;
const FAMILY: usize = 16;
const SELECTIONS: [&str; 3] = ["session.start", "session.resume", "session.model_change"];
const SUBAGENTS: [&str; 3] = ["subagent.started", "subagent.completed", "subagent.failed"];
const BOUNDARIES: [&str; 5] = [
    "session.auto_mode_resolved",
    "session.compaction_complete",
    "session.truncation",
    "session.context_cleared",
    "session.shutdown",
];

thread_local! {
    static BACK_LIMIT: Cell<Option<u64>> = const { Cell::new(None) };
}

#[derive(Debug)]
pub(in crate::agents) struct Session {
    home: PathBuf,
    pub(in crate::agents) path: PathBuf,
    pub(in crate::agents) id: String,
    pub(in crate::agents) resumable: bool,
    pub(in crate::agents) prompted: Option<SystemTime>,
    pub(in crate::agents) activity: Option<Activity>,
    pub(in crate::agents) context: Option<Context>,
    pub(in crate::agents) compaction: Compaction,
    pub(in crate::agents) counted: Vec<Record>,
    pub(in crate::agents) subagents: usize,
    held: bool,
    seeded: bool,
    tail: Tail,
    fold: Fold,
    working: Option<bool>,
}

struct Found {
    home: PathBuf,
    path: PathBuf,
    id: String,
    held: bool,
}

pub(in crate::agents) fn follow(pid: i32, args: &[String], previous: Option<Session>) -> Option<Session> {
    let Found { home, path, id, held } = locate(pid, args)?;
    let mut session = match previous {
        Some(session) if session.path == path => session,
        _ => Session::new(home, path, id),
    };
    session.held = held;
    session.refresh();
    Some(session)
}

#[cfg(test)]
pub(in crate::agents) fn look(pid: i32, args: &[String]) -> Option<Session> {
    follow(pid, args, None)
}

fn locate(pid: i32, args: &[String]) -> Option<Found> {
    let mut family = vec![pid];
    let mut found = held_by(pid);
    if found.is_empty() {
        for child in process::descendants(pid).into_iter().take(FAMILY * 16) {
            if family.len() < FAMILY && is_copilot(&process::args(child)) {
                family.push(child);
                found.extend(held_by(child));
            }
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);
    if found.len() > 1 {
        return None;
    }
    found.pop().or_else(|| named(pid, args, &family))
}

fn is_copilot(args: &[String]) -> bool {
    discovery::named(args, super::NAME) || args.iter().any(|arg| super::runs_as(arg))
}

fn held_by(pid: i32) -> Vec<Found> {
    let Some(home) = discovery::home(pid, "COPILOT_HOME", "", ".copilot") else { return Vec::new() };
    let wanted = format!("inuse.{pid}.hold");
    let sessions = home.join("session-state");
    process::open_files(pid)
        .into_iter()
        .filter(|file| file.file_name().is_some_and(|name| *name == *wanted))
        .filter_map(|file| {
            let path = file.parent()?.to_path_buf();
            let id = plain(path.file_name()?.to_str()?)?;
            (path.parent()? == sessions).then(|| Found { home: home.clone(), path, id, held: true })
        })
        .collect()
}

fn plain(id: &str) -> Option<String> {
    let valid = !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    valid.then(|| id.to_string())
}

fn named(pid: i32, args: &[String], family: &[i32]) -> Option<Found> {
    let id = plain(&resumed(args)?)?;
    let home = discovery::home(pid, "COPILOT_HOME", "", ".copilot")?;
    let path = home.join("session-state").join(&id);
    if !path.is_dir() || owned_elsewhere(&path, family) {
        return None;
    }
    Some(Found { home, path, id, held: false })
}

fn resumed(args: &[String]) -> Option<String> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        for flag in ["--resume", "-r", "--session-id"] {
            if let Some(value) = arg.strip_prefix(flag).and_then(|rest| rest.strip_prefix('=')) {
                return Some(value.to_string());
            }
            if arg == flag {
                return args.next().filter(|value| !value.starts_with('-')).cloned();
            }
        }
    }
    None
}

fn owned_elsewhere(path: &Path, family: &[i32]) -> bool {
    std::fs::read_dir(path).into_iter().flatten().flatten().any(|entry| {
        let name = entry.file_name();
        let owner =
            name.to_str().and_then(|name| name.strip_prefix("inuse.")?.strip_suffix(".lock")?.parse::<i32>().ok());
        owner.is_some_and(|owner| !family.contains(&owner) && process::alive(owner))
    })
}

fn back_limit() -> u64 {
    BACK_LIMIT.with(Cell::get).unwrap_or(BACK_DEFAULT)
}

#[cfg(test)]
pub(in crate::agents) fn limit_back(limit: Option<u64>) {
    BACK_LIMIT.with(|cell| cell.set(limit));
}

fn seed(path: &Path) -> Vec<Value> {
    let Ok(Some(lines)) = message::lines_within(path, back_limit()) else { return Vec::new() };
    let mut found = Vec::new();
    let (mut selected, mut used, mut said) = (false, false, false);
    for line in lines.flatten() {
        let Ok(event) = serde_json::from_slice::<Value>(&line) else { continue };
        let kind = event["type"].as_str().unwrap_or_default();
        let keep = if SELECTIONS.contains(&kind) {
            selected = true;
            true
        } else if BOUNDARIES.contains(&kind) || SUBAGENTS.contains(&kind) {
            true
        } else if kind == "session.usage_record" {
            events::is_main(&event["data"]["usage"]) && !std::mem::replace(&mut used, true)
        } else if kind == "assistant.message" || (kind == "user.message" && events::is_prompt(&event["data"])) {
            !std::mem::replace(&mut said, true)
        } else {
            false
        };
        if keep {
            found.push(event);
        }
        if selected && used && said {
            break;
        }
    }
    found.reverse();
    found
}

fn presence(home: &Path, id: &str, before: Option<bool>) -> Option<bool> {
    let text = std::fs::read_to_string(home.join("open-sessions-state.json")).ok()?;
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => value[id]["working"].as_bool(),
        Err(_) => before,
    }
}

impl Session {
    fn new(home: PathBuf, path: PathBuf, id: String) -> Self {
        Self {
            path,
            id,
            activity: None,
            context: None,
            compaction: Compaction::default(),
            counted: Vec::new(),
            subagents: 0,
            prompted: None,
            resumable: false,
            home,
            held: false,
            seeded: false,
            tail: Tail::default(),
            fold: Fold::default(),
            working: None,
        }
    }

    fn refresh(&mut self) {
        self.working = presence(&self.home, &self.id, self.working);
        self.read_events(&self.path.join("events.jsonl"));
        self.activity = self.current_activity();
        self.subagents = self.fold.subagents(SystemTime::now());
        self.context = self.fold.context();
        self.compaction = self.fold.compaction();
        self.prompted = self.prompted.max(self.fold.prompted);
        self.resumable = self.fold.resumable;
        self.counted.append(&mut self.fold.counted);
    }

    fn read_events(&mut self, file: &Path) {
        let Ok(batch) = self.tail.read(file) else {
            self.fold = Fold::default();
            self.seeded = false;
            return;
        };
        if batch.reset || !self.seeded {
            self.fold = Fold::default();
            for event in seed(file) {
                self.fold.apply(&event);
            }
            self.fold.counted.clear();
            self.seeded = true;
        }
        for event in jsonl::values(&batch.bytes) {
            self.fold.apply(&event);
        }
    }

    fn current_activity(&self) -> Option<Activity> {
        if !self.held {
            return Some(Activity::Idle);
        }
        match self.working {
            Some(true) if self.fold.waiting() => Some(Activity::Waiting),
            Some(true) if self.fold.shell_only() => Some(Activity::Shell),
            Some(true) => Some(Activity::Working),
            Some(false) => Some(Activity::Idle),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests;
