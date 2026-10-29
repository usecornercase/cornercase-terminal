use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agents::context::counting::{Record as CountRecord, Usage};
use crate::agents::context::{Compaction, Context, discovery, percent};
use crate::agents::stats::Tokens;
use crate::process;

const STORE: [&str; 3] = ["logs", "session", "unified"];
const RUNNING: [&str; 2] = ["reserved", "running"];

#[derive(Debug)]
pub(in crate::agents) struct Session {
    pub(in crate::agents) path: PathBuf,
    pub(in crate::agents) id: String,
    pub(in crate::agents) context: Option<Context>,
    pub(in crate::agents) compaction: Compaction,
    pub(in crate::agents) subagents: usize,
    pub(in crate::agents) resumable: bool,
    pub(in crate::agents) prompted: Option<SystemTime>,
    pub(in crate::agents) counted: Vec<CountRecord>,
    compactions: u64,
    seeded: bool,
    tokens: Option<Tokens>,
    last_turn: Option<String>,
}

pub(in crate::agents) fn follow(
    pid: i32,
    args: &[String],
    previous: Option<Session>,
    home: Option<PathBuf>,
) -> Option<Session> {
    let home = match home {
        Some(home) => home,
        None => discovery::home(pid, "VIBE_HOME", "", ".vibe")?,
    };
    let id = lease(&home, pid).or_else(|| resumed(args))?;
    read(&home, id, previous)
}

fn read(home: &Path, id: String, previous: Option<Session>) -> Option<Session> {
    let path = STORE.iter().fold(home.to_path_buf(), |path, part| path.join(part)).join(&id);
    if !path.is_dir() {
        return None;
    }
    let mut session = match previous {
        Some(session) if session.path == path => session,
        _ => Session {
            path,
            id,
            context: None,
            compaction: Compaction::default(),
            subagents: 0,
            resumable: false,
            prompted: None,
            counted: Vec::new(),
            compactions: 0,
            seeded: false,
            tokens: None,
            last_turn: None,
        },
    };
    session.refresh(SystemTime::now()).then_some(session)
}

fn lease(home: &Path, pid: i32) -> Option<String> {
    let entries = std::fs::read_dir(home.join("logs/session/active")).ok()?;
    let mut family = vec![pid];
    let mut found: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(|name| name.ends_with(".lock.json")) {
            continue;
        }
        let Some(lock) = json(&entry.path()) else { continue };
        let Some(owner) = lock["process_id"].as_i64().and_then(|owner| i32::try_from(owner).ok()) else {
            continue;
        };
        let Some(id) = lock["session_id"].as_str().filter(|id| plain(id)) else { continue };
        if owner != pid && family.len() == 1 {
            family.extend(process::descendants(pid));
        }
        if family.contains(&owner) {
            found.push(id.to_string());
        }
    }
    found.sort();
    found.dedup();
    (found.len() == 1).then(|| found.remove(0))
}

fn resumed(args: &[String]) -> Option<String> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if let Some(id) = arg.strip_prefix("--resume=") {
            return plain(id).then(|| id.to_string());
        }
        if arg == "--resume" {
            return args.next().filter(|id| plain(id)).cloned();
        }
    }
    None
}

fn plain(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}

fn moment(ms: u64) -> SystemTime {
    UNIX_EPOCH + std::time::Duration::from_millis(ms)
}

impl Session {
    fn refresh(&mut self, now: SystemTime) -> bool {
        let Some(meta) = json(&self.path.join("meta.json")) else {
            self.context = None;
            return false;
        };
        let (projection, runtime, checkpoint) = self.snapshots();
        let turn = projection.as_ref().and_then(latest_turn);
        let model = model_of(&meta, runtime.as_ref());
        let session = projection.as_ref().map(|projection| &projection["snapshot"]["session"]);
        let used = session.and_then(context_usage);
        let window = runtime.as_ref().and_then(window_of);
        let compacted = self.observe(checkpoint.as_ref(), turn, now);
        let percent = match (used, window) {
            (Some(used), Some(window)) if !self.compaction.hides => Some(percent(used, window)),
            _ => None,
        };
        self.context = model.map(|model| Context { model: model.to_string(), percent });
        self.subagents = running_subagents(runtime.as_ref());
        self.resumable = turn.is_some();
        self.prompt(turn, compacted);
        if !compacted {
            self.prompted = turn.and_then(|turn| turn["startedAt"].as_u64()).map(moment).or(self.prompted);
        }
        self.record_tokens(session, now);
        true
    }

    fn snapshots(&self) -> (Option<Value>, Option<Value>, Option<Value>) {
        let generation = json(&self.path.join("CURRENT"))
            .and_then(|current| {
                current["generation"].as_str().filter(|generation| plain(generation)).map(str::to_string)
            })
            .map(|generation| self.path.join("generations").join(generation));
        let of = |name: &str| generation.as_ref().and_then(|dir| json(&dir.join(name)));
        (of("projection-state.json"), of("runtime-state.json"), of("checkpoint.json"))
    }

    fn observe(&mut self, checkpoint: Option<&Value>, turn: Option<&Value>, now: SystemTime) -> bool {
        let count = checkpoint.and_then(|checkpoint| checkpoint["compaction_count"].as_u64());
        if !std::mem::replace(&mut self.seeded, true) {
            self.compactions = count.unwrap_or_default();
            return false;
        }
        let grew = count.is_some_and(|count| count > self.compactions);
        if grew {
            self.compaction.happened(now);
        }
        self.compactions = self.compactions.max(count.unwrap_or_default());
        let completed = turn.and_then(|turn| turn["completedAt"].as_u64());
        if self.compaction.hides
            && completed.is_some_and(|completed| completed > millis(self.compaction.at.unwrap_or(now)))
        {
            self.compaction.hides = false;
        }
        grew
    }

    fn prompt(&mut self, turn: Option<&Value>, compacted: bool) {
        let Some(id) = turn.and_then(|turn| turn["id"].as_str()) else { return };
        if self.last_turn.as_deref() == Some(id) {
            return;
        }
        if !compacted && let Some(started) = turn.and_then(|turn| turn["startedAt"].as_u64()) {
            self.counted.push(CountRecord { at: moment(started), id: id.to_string(), usage: Usage::Prompt });
        }
        self.last_turn = Some(id.to_string());
    }

    fn record_tokens(&mut self, session: Option<&Value>, now: SystemTime) {
        let Some(total) = session.and_then(cumulative_usage) else { return };
        if self.tokens == Some(total) {
            return;
        }
        self.counted.push(CountRecord {
            at: now,
            id: "usage".into(),
            usage: Usage::Cumulative { total, last: Tokens::default() },
        });
        self.tokens = Some(total);
    }
}

fn latest_turn(projection: &Value) -> Option<&Value> {
    projection["snapshot"]["latestTurn"].as_object().map(|_| &projection["snapshot"]["latestTurn"])
}

fn model_of<'a>(meta: &'a Value, runtime: Option<&'a Value>) -> Option<&'a str> {
    runtime
        .and_then(|runtime| runtime["session_metadata"]["active_model"].as_str())
        .or_else(|| meta["config"]["active_model"].as_str())
        .filter(|model| !model.trim().is_empty())
}

fn window_of(runtime: &Value) -> Option<u64> {
    let context = &runtime["core_settings"]["context"];
    context["model_facts"]["compaction_threshold"]
        .as_u64()
        .or_else(|| context["compaction"]["token_threshold"].as_u64())
        .filter(|window| *window > 0)
}

fn cumulative_usage(session: &Value) -> Option<Tokens> {
    let usage = &session["tokenUsage"];
    if usage.is_null() {
        return None;
    }
    let input = usage["inputTokens"].as_u64()?;
    let cached = usage["cachedInputTokens"].as_u64().unwrap_or_default();
    let output = usage["outputTokens"].as_u64().unwrap_or_default();
    Some(Tokens { input: input.saturating_sub(cached), out: output, cache_read: cached, cache_write: 0 })
}

fn context_usage(session: &Value) -> Option<u64> {
    let usage = &session["contextUsage"];
    if usage.is_null() {
        return None;
    }
    let input = usage["inputTokens"].as_u64()?;
    let output = usage["outputTokens"].as_u64().unwrap_or_default();
    Some(input.saturating_add(output))
}

fn running_subagents(runtime: Option<&Value>) -> usize {
    let Some(children) = runtime.and_then(|runtime| runtime["subagents"]["children"].as_object()) else {
        return 0;
    };
    children
        .values()
        .filter(|child| child["state"]["type"].as_str().is_some_and(|state| RUNNING.contains(&state)))
        .count()
}

#[cfg(test)]
pub(in crate::agents) fn look(home: &Path, id: &str) -> Option<Session> {
    read(home, id.to_string(), None)
}

#[cfg(test)]
pub(in crate::agents) fn look_after(home: &Path, id: &str, previous: Session) -> Option<Session> {
    read(home, id.to_string(), Some(previous))
}

#[cfg(test)]
mod tests;
