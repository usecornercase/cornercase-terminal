use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agents::activity::Activity;
use crate::agents::context::counting::{self, Usage};
use crate::agents::context::message::{self, Said};
use crate::agents::context::{self, Context, jsonl};
use crate::agents::stats::Tokens;
use crate::error::Result;
use crate::process;
use crate::time::Stamp;

const ASKS: [&str; 2] = ["ask_user_question", "exit_plan_mode"];

#[derive(Debug)]
pub(in crate::agents) struct Session {
    pub(in crate::agents) path: PathBuf,
    pub(in crate::agents) id: String,
    pub(in crate::agents) activity: Activity,
    pub(in crate::agents) turn_open: bool,
    pub(in crate::agents) context: Option<Context>,
    pub(in crate::agents) counted: Vec<counting::Record>,
    pub(in crate::agents) extras: Vec<(String, Vec<counting::Record>)>,
    pub(in crate::agents) prompted: Option<SystemTime>,
    pub(in crate::agents) resumable: bool,
}

struct Turn {
    at: SystemTime,
    model: Option<String>,
    number: Option<u64>,
    primary: bool,
}

struct Reply {
    at: SystemTime,
    id: String,
    tokens: Tokens,
    input_total: u64,
    calls: Option<u64>,
    model: Option<String>,
    primary: bool,
}

pub(in crate::agents) fn look(pid: i32) -> Option<Session> {
    let home = grok_home(pid)?;
    let (id, cwd) = row(&home, pid)?;
    let dir = locate(&home, &id, &cwd)?;
    Some(read_session(&dir, &id))
}

pub(in crate::agents) fn encode_path(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(hex(byte >> 4));
            out.push(hex(byte & 0x0f));
        }
    }
    out
}

pub(in crate::agents) fn last_message(dir: &Path) -> Result<Option<Said>> {
    let chat = dir.join("chat_history.jsonl");
    let Some(lines) = message::lines(&chat)? else { return Ok(None) };
    let mut text = None;
    for line in lines {
        let line = line.map_err(|e| message::unreadable(&chat, &e))?;
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if kind_of(&entry) == Some("assistant")
            && let Some(found) = entry["content"].as_str().map(str::trim).filter(|text| !text.is_empty())
        {
            text = Some(found.to_string());
            break;
        }
    }
    let Some(text) = text else { return Ok(None) };
    Ok(Some(Said { text, written: written(dir) }))
}

fn grok_home(pid: i32) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name: &str| {
        env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, value)| PathBuf::from(value))
    };
    let path = var("GROK_HOME").or_else(|| var("HOME").map(|home| home.join(".grok")))?;
    let path = if path.is_absolute() { path } else { process::cwd(pid)?.join(path) };
    path.canonicalize().ok()
}

fn row(home: &Path, pid: i32) -> Option<(String, String)> {
    let text = std::fs::read_to_string(home.join("active_sessions.json")).ok()?;
    let rows: Vec<Value> = serde_json::from_str(&text).ok()?;
    let started = process::started(pid);
    for entry in rows {
        if entry["pid"].as_i64() != Some(i64::from(pid)) {
            continue;
        }
        if let (Some(actual), Some(recorded)) = (started, entry["pid_start"].as_u64())
            && actual != recorded
        {
            continue;
        }
        let id = entry["session_id"].as_str()?.to_string();
        let cwd = entry["cwd"].as_str()?.to_string();
        return Some((id, cwd));
    }
    None
}

fn locate(home: &Path, id: &str, cwd: &str) -> Option<PathBuf> {
    let sessions = home.join("sessions");
    let direct = sessions.join(encode_path(cwd)).join(id);
    if direct.is_dir() {
        return Some(direct);
    }
    let wanted = Path::new(cwd).canonicalize().unwrap_or_else(|_| PathBuf::from(cwd));
    for entry in std::fs::read_dir(&sessions).ok()?.flatten() {
        let slug = entry.path();
        if !slug.is_dir() {
            continue;
        }
        let dir = slug.join(id);
        if dir.is_dir() && owns(&slug, &dir, &wanted) {
            return Some(dir);
        }
    }
    None
}

fn owns(slug: &Path, dir: &Path, cwd: &Path) -> bool {
    if let Ok(bytes) = std::fs::read(slug.join(".cwd")) {
        let text = String::from_utf8_lossy(&bytes);
        let stored = Path::new(text.trim_end_matches(['\n', '\r']));
        if same_dir(stored, cwd) {
            return true;
        }
    }
    summary_cwd(dir).is_some_and(|stored| same_dir(Path::new(&stored), cwd))
}

fn same_dir(stored: &Path, cwd: &Path) -> bool {
    let stored = stored.canonicalize().unwrap_or_else(|_| stored.to_path_buf());
    stored == cwd
}

fn summary_cwd(dir: &Path) -> Option<String> {
    let summary = serde_json::from_str::<Value>(&std::fs::read_to_string(dir.join("summary.json")).ok()?).ok()?;
    summary["info"]["cwd"].as_str().map(str::to_string)
}

fn read_session(dir: &Path, id: &str) -> Session {
    let events = values(&dir.join("events.jsonl"));
    let updates = values(&dir.join("updates.jsonl"));
    let chat = values(&dir.join("chat_history.jsonl"));
    let summary = json_file(&dir.join("summary.json"));
    let signals = json_file(&dir.join("signals.json"));
    let (activity, turn_open, turns) = fold(&events);
    let (replies, compact) = replies_of(&updates, &turns);
    let (mut counted, prompted, resumable) = prompts(&chat, &turns, &updates);
    let mut extras = side_records(id, &replies);
    extras.extend(child_records(dir));
    for reply in &replies {
        if reply.primary && !reply.tokens.is_empty() {
            counted.push(token_record(reply));
        }
    }
    Session {
        path: dir.to_path_buf(),
        id: id.to_string(),
        activity,
        turn_open,
        context: context_of(&summary, &signals, &turns, &replies, compact),
        counted,
        extras,
        prompted,
        resumable,
    }
}

fn fold(events: &[Value]) -> (Activity, bool, Vec<Turn>) {
    let mut turn_open = false;
    let mut phase = String::new();
    let mut unresolved = false;
    let mut asks = 0_u32;
    let mut turns = Vec::new();
    for event in events {
        match event["type"].as_str() {
            Some("turn_started") => {
                turn_open = true;
                phase.clear();
                unresolved = false;
                asks = 0;
                if let Some(at) = event_time(event) {
                    turns.push(Turn {
                        at,
                        model: event["model_id"].as_str().map(str::to_string),
                        number: event["turn_number"].as_u64(),
                        primary: event["session_relationship"].as_str().is_none_or(|kind| kind == "primary"),
                    });
                }
            }
            Some("turn_ended") => {
                turn_open = false;
                phase.clear();
                unresolved = false;
                asks = 0;
            }
            Some("phase_changed") => phase = event["phase"].as_str().unwrap_or_default().to_string(),
            Some("permission_requested") => unresolved = true,
            Some("permission_resolved") => unresolved = false,
            Some("tool_started") if asks_you(event) => asks = asks.saturating_add(1),
            Some("tool_completed") if asks_you(event) => asks = asks.saturating_sub(1),
            _ => {}
        }
    }
    let activity = if !turn_open {
        Activity::Idle
    } else if phase == "permission_prompt" || unresolved || asks > 0 {
        Activity::Waiting
    } else {
        Activity::Working
    };
    (activity, turn_open, turns)
}

fn asks_you(event: &Value) -> bool {
    event["tool_name"].as_str().is_some_and(|name| ASKS.contains(&name))
}

fn replies_of(updates: &[Value], turns: &[Turn]) -> (Vec<Reply>, Option<SystemTime>) {
    let mut compact = None;
    let mut found = Vec::new();
    for update in updates {
        let body = &update["params"]["update"];
        let at = unix_time(&update["timestamp"]);
        match body["sessionUpdate"].as_str() {
            Some("auto_compact_completed" | "compaction_checkpoint") => compact = compact.max(at),
            Some("turn_completed") => {
                let Some(at) = at else { continue };
                found.push(reply(body, at, turns));
            }
            _ => {}
        }
    }
    (found, compact)
}

fn reply(body: &Value, at: SystemTime, turns: &[Turn]) -> Reply {
    let usage = &body["usage"];
    let input_total = usage["inputTokens"].as_u64().unwrap_or(0);
    let cached = usage["cachedReadTokens"].as_u64().unwrap_or(0);
    let turn = turns.iter().rev().find(|turn| turn.at <= at);
    Reply {
        at,
        id: body["prompt_id"].as_str().map_or_else(|| Stamp(at).to_string(), str::to_string),
        tokens: Tokens {
            input: input_total.saturating_sub(cached),
            out: usage["outputTokens"].as_u64().unwrap_or(0),
            cache_read: cached,
            cache_write: usage["cacheCreationTokens"].as_u64().unwrap_or(0),
        },
        input_total,
        calls: usage["modelCalls"].as_u64(),
        model: turn.and_then(|turn| turn.model.clone()),
        primary: turn.is_none_or(|turn| turn.primary),
    }
}

fn context_of(
    summary: &Value,
    signals: &Value,
    turns: &[Turn],
    replies: &[Reply],
    compact: Option<SystemTime>,
) -> Option<Context> {
    let model = summary["current_model_id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| turns.iter().rev().find_map(|turn| turn.model.clone()))?;
    let window = signals["contextWindowTokens"]
        .as_u64()
        .filter(|n| *n > 0)
        .or_else(|| summary["context_window"].as_u64())
        .unwrap_or(0);
    Some(Context { model: model.clone(), percent: occupancy(&model, window, replies, compact, signals) })
}

fn occupancy(model: &str, window: u64, replies: &[Reply], compact: Option<SystemTime>, signals: &Value) -> Option<u16> {
    if window == 0 {
        return None;
    }
    let fresh = |at: SystemTime| compact.is_none_or(|cut| at > cut);
    let same = |reply: &Reply| reply.model.as_deref().is_none_or(|id| id == model);
    if let Some(reply) = replies.iter().rev().find(|reply| {
        fresh(reply.at) && same(reply) && reply.calls.is_some_and(|calls| calls <= 1) && reply.input_total <= window
    }) {
        return Some(context::percent(reply.input_total, window));
    }
    let newest = replies.last();
    let invalidated = match newest {
        Some(reply) => !fresh(reply.at) || reply.model.as_deref().is_some_and(|id| id != model),
        None => compact.is_some(),
    };
    if invalidated {
        return None;
    }
    let used = signals["contextTokensUsed"].as_u64().unwrap_or(0);
    (used > 0 && used <= window).then(|| context::percent(used, window))
}

fn prompts(chat: &[Value], turns: &[Turn], updates: &[Value]) -> (Vec<counting::Record>, Option<SystemTime>, bool) {
    let chunks = chunk_times(updates);
    let mut records = Vec::new();
    let mut resumable = false;
    for line in chat {
        if line["synthetic_reason"].as_str() == Some("compaction_meta") {
            resumable = true;
        }
        if kind_of(line) == Some("assistant") && line["content"].is_string() {
            resumable = true;
        }
        if kind_of(line) != Some("user") || line.get("synthetic_reason").is_some() {
            continue;
        }
        let Some(index) = line["prompt_index"].as_u64() else { continue };
        resumable = true;
        let Some(at) = prompt_time(index, turns, &chunks) else { continue };
        records.push(counting::Record { at, id: format!("prompt-{index}"), usage: Usage::Prompt });
    }
    let prompted = records.iter().map(|record| record.at).max();
    (records, prompted, resumable)
}

fn chunk_times(updates: &[Value]) -> Vec<(Option<u64>, SystemTime)> {
    updates
        .iter()
        .filter(|update| update["params"]["update"]["sessionUpdate"] == "user_message_chunk")
        .filter_map(|update| {
            let at = unix_time(&update["timestamp"])?;
            let index = update["params"]["update"]["_meta"]["promptIndex"].as_u64();
            Some((index, at))
        })
        .collect()
}

fn prompt_time(index: u64, turns: &[Turn], chunks: &[(Option<u64>, SystemTime)]) -> Option<SystemTime> {
    turns
        .iter()
        .find(|turn| turn.primary && turn.number == Some(index))
        .map(|turn| turn.at)
        .or_else(|| chunks.iter().find(|(n, _)| *n == Some(index)).map(|(_, at)| *at))
        .or_else(|| chunks.get(usize::try_from(index).ok()?).map(|(_, at)| *at))
}

fn side_records(id: &str, replies: &[Reply]) -> Vec<(String, Vec<counting::Record>)> {
    let records: Vec<_> =
        replies.iter().filter(|reply| !reply.primary && !reply.tokens.is_empty()).map(token_record).collect();
    if records.is_empty() { Vec::new() } else { vec![(format!("{id}/side"), records)] }
}

fn child_records(dir: &Path) -> Vec<(String, Vec<counting::Record>)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() || !path.join("updates.jsonl").is_file() {
                return None;
            }
            let (replies, _) = replies_of(&values(&path.join("updates.jsonl")), &[]);
            let records: Vec<_> = replies.iter().filter(|reply| !reply.tokens.is_empty()).map(token_record).collect();
            (!records.is_empty()).then(|| (path.to_string_lossy().into_owned(), records))
        })
        .collect()
}

fn token_record(reply: &Reply) -> counting::Record {
    counting::Record { at: reply.at, id: reply.id.clone(), usage: Usage::Completed(reply.tokens) }
}

fn written(dir: &Path) -> Option<String> {
    let updates = newest_update(&dir.join("updates.jsonl"), "agent_message_chunk");
    let ended = newest_event(&dir.join("events.jsonl"));
    updates.max(ended).map(|at| Stamp(at).to_string())
}

fn newest_update(path: &Path, kind: &str) -> Option<SystemTime> {
    let lines = message::lines(path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if entry["params"]["update"]["sessionUpdate"].as_str() == Some(kind) {
            return unix_time(&entry["timestamp"]);
        }
    }
    None
}

fn newest_event(path: &Path) -> Option<SystemTime> {
    let lines = message::lines(path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if entry["type"].as_str() == Some("turn_ended") {
            return event_time(&entry);
        }
    }
    None
}

fn values(path: &Path) -> Vec<Value> {
    let Ok(batch) = jsonl::Tail::default().read(path) else { return Vec::new() };
    batch
        .bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

fn json_file(path: &Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null)
}

fn kind_of(entry: &Value) -> Option<&str> {
    entry["type"].as_str().or_else(|| entry["role"].as_str())
}

fn event_time(entry: &Value) -> Option<SystemTime> {
    entry["ts"].as_str().and_then(Stamp::parse)
}

fn unix_time(value: &Value) -> Option<SystemTime> {
    let secs = value.as_u64().or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))?;
    UNIX_EPOCH.checked_add(Duration::from_secs(secs))
}

fn hex(n: u8) -> char {
    char::from(b"0123456789ABCDEF"[usize::from(n)])
}
