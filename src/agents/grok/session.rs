use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agents::activity::Activity;
use crate::agents::context::counting::{self, Usage};
use crate::agents::context::jsonl::{self, Tail};
use crate::agents::context::message::{self, Said};
use crate::agents::context::{self, Context};
use crate::agents::stats::Tokens;
use crate::error::Result;
use crate::process;
use crate::time::Stamp;

const ASKS: [&str; 2] = ["ask_user_question", "exit_plan_mode"];
const TAIL_BYTES: u64 = 1024 * 1024;
const TURNS: usize = 64;

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
    events: Tail,
    updates: Tail,
    chat: Tail,
    start: Start,
    phase: String,
    pending: HashMap<String, u32>,
    asks: u32,
    turns: Vec<Turn>,
    primary: bool,
    compact: Option<SystemTime>,
    newest: Option<Reply>,
    seen_model: Option<String>,
    seen_window: u64,
    boundary: Option<SystemTime>,
    window: u64,
    model: Option<String>,
    signals_used: u64,
    signals_model: Option<String>,
    pending_chunks: VecDeque<Chunk>,
    waiting_humans: u32,
    seen_prompts: HashSet<String>,
    children: Vec<Child>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    New,
    Seen,
}

#[derive(Debug)]
struct Turn {
    at: SystemTime,
    model: Option<String>,
    primary: bool,
}

#[derive(Debug)]
struct Reply {
    at: SystemTime,
    id: String,
    tokens: Tokens,
    input_total: u64,
    calls: Option<u64>,
    model: Option<String>,
    primary: bool,
}

#[derive(Debug)]
struct Chunk {
    at: SystemTime,
    id: String,
    primary: bool,
}

#[derive(Debug)]
struct Child {
    path: PathBuf,
    tail: Tail,
}

#[cfg(test)]
pub(in crate::agents) fn look(pid: i32) -> Option<Session> {
    follow(pid, None)
}

pub(in crate::agents) fn follow(pid: i32, previous: Option<Session>) -> Option<Session> {
    let home = grok_home(pid)?;
    let (id, cwd) = row(&home, pid)?;
    let dir = locate(&home, &id, &cwd)?;
    let mut session = match previous {
        Some(prev) if prev.id == id && prev.path == dir => prev,
        _ => Session::open(dir, id),
    };
    session.refresh();
    Some(session)
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
    Ok(Some(Said { text: text.clone(), written: written(dir, &text) }))
}

impl Session {
    fn open(path: PathBuf, id: String) -> Self {
        Self {
            path,
            id,
            activity: Activity::Idle,
            turn_open: false,
            context: None,
            counted: Vec::new(),
            extras: Vec::new(),
            prompted: None,
            resumable: false,
            events: Tail::default(),
            updates: Tail::default(),
            chat: Tail::default(),
            start: Start::New,
            phase: String::new(),
            pending: HashMap::new(),
            asks: 0,
            turns: Vec::new(),
            primary: true,
            compact: None,
            newest: None,
            seen_model: None,
            seen_window: 0,
            boundary: None,
            window: 0,
            model: None,
            signals_used: 0,
            signals_model: None,
            pending_chunks: VecDeque::new(),
            waiting_humans: 0,
            seen_prompts: HashSet::new(),
            children: Vec::new(),
        }
    }

    fn refresh(&mut self) {
        let opening = self.start == Start::New;
        self.start = Start::Seen;
        self.read_events(opening);
        let partial = opening && self.prompts_outside_tail();
        let replayed = self.read_updates(partial) || self.read_chat(partial);
        if partial || replayed {
            self.pending_chunks.clear();
            self.waiting_humans = 0;
            self.recover_prompt();
        }
        if opening {
            self.recover_resumable();
        }
        self.note_model();
        self.context = self.model.clone().map(|model| Context { model, percent: self.occupancy() });
        self.activity = self.activity_of();
        self.read_children();
    }

    fn prompts_outside_tail(&self) -> bool {
        exceeds(&self.path.join("updates.jsonl")) || exceeds(&self.path.join("chat_history.jsonl"))
    }

    fn read_events(&mut self, opening: bool) {
        let path = self.path.join("events.jsonl");
        let Some(batch) = read_tail(&mut self.events, &path) else { return };
        if opening || batch.reset {
            self.turn_open = false;
            self.phase.clear();
            self.pending.clear();
            self.asks = 0;
            self.turns.clear();
            self.primary = true;
            if let Some(life) = newest_life(&path) {
                self.turn_open = life.open;
                self.primary = life.primary;
            }
        }
        for event in lines_of(&batch.bytes) {
            self.apply_event(&event);
        }
    }

    fn read_updates(&mut self, partial: bool) -> bool {
        let path = self.path.join("updates.jsonl");
        let Some(batch) = read_tail(&mut self.updates, &path) else { return false };
        let replay = batch.reset && exceeds(&path);
        if batch.reset {
            self.clear_prompts();
            self.compact = None;
            self.newest = None;
        }
        for update in lines_of(&batch.bytes) {
            self.apply_update(&update, partial || replay);
        }
        replay
    }

    fn read_chat(&mut self, partial: bool) -> bool {
        let path = self.path.join("chat_history.jsonl");
        let Some(batch) = read_tail(&mut self.chat, &path) else { return false };
        let replay = batch.reset && exceeds(&path);
        if batch.reset {
            self.pending_chunks.clear();
            self.waiting_humans = 0;
            self.resumable = false;
        }
        for line in lines_of(&batch.bytes) {
            self.apply_chat(&line, partial || replay);
        }
        replay
    }

    fn apply_event(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("turn_started") => {
                self.turn_open = true;
                self.phase.clear();
                self.pending.clear();
                self.asks = 0;
                self.primary = is_primary(event);
                if let Some(at) = event_time(event) {
                    self.turns.push(Turn {
                        at,
                        model: event["model_id"].as_str().map(str::to_string),
                        primary: self.primary,
                    });
                    if self.turns.len() > TURNS {
                        self.turns.remove(0);
                    }
                }
            }
            Some("turn_ended") => {
                self.turn_open = false;
                self.phase.clear();
                self.pending.clear();
                self.asks = 0;
                self.primary = true;
            }
            Some("phase_changed") => self.phase = event["phase"].as_str().unwrap_or_default().to_string(),
            Some("permission_requested") => *self.pending.entry(tool_name(event)).or_insert(0) += 1,
            Some("permission_resolved") => {
                let name = tool_name(event);
                if let Some(left) = self.pending.get_mut(&name) {
                    *left = left.saturating_sub(1);
                }
            }
            Some("tool_started") if asks_you(event) => self.asks = self.asks.saturating_add(1),
            Some("tool_completed") if asks_you(event) => self.asks = self.asks.saturating_sub(1),
            _ => {}
        }
    }

    fn apply_update(&mut self, update: &Value, skip_prompts: bool) {
        let body = &update["params"]["update"];
        let at = unix_time(&update["timestamp"]);
        match body["sessionUpdate"].as_str() {
            Some("auto_compact_completed" | "compaction_checkpoint") => self.compact = self.compact.max(at),
            Some("turn_completed") => {
                let Some(at) = at else { return };
                let reply = self.reply(body, at);
                if !reply.tokens.is_empty() {
                    let record = token_record(&reply);
                    if reply.primary {
                        self.counted.push(record);
                    } else {
                        self.push_extra(format!("{}/side", self.id), record);
                    }
                }
                self.newest = Some(reply);
            }
            Some("user_message_chunk") if !skip_prompts => self.note_chunk(update, body),
            _ => {}
        }
    }

    fn apply_chat(&mut self, line: &Value, skip_prompts: bool) {
        if line["synthetic_reason"].as_str() == Some("compaction_meta") {
            self.resumable = true;
        }
        if kind_of(line) == Some("assistant") && line["content"].is_string() {
            self.resumable = true;
        }
        if skip_prompts || kind_of(line) != Some("user") || line.get("synthetic_reason").is_some() {
            return;
        }
        if line["prompt_index"].as_u64().is_none() {
            return;
        }
        self.resumable = true;
        if let Some(chunk) = self.pending_chunks.pop_front() {
            self.emit_prompt(chunk);
        } else {
            self.waiting_humans = self.waiting_humans.saturating_add(1);
        }
    }

    fn note_chunk(&mut self, update: &Value, body: &Value) {
        let text = body["content"]["text"].as_str().unwrap_or_default();
        if reminder(text) || body["_meta"]["promptIndex"].as_u64().is_none() {
            return;
        }
        let Some(chunk) = chunk_of(update, self.primary) else { return };
        if self.waiting_humans > 0 {
            self.waiting_humans -= 1;
            self.emit_prompt(chunk);
        } else {
            self.pending_chunks.push_back(chunk);
        }
    }

    fn emit_prompt(&mut self, chunk: Chunk) {
        if !chunk.primary || !self.seen_prompts.insert(chunk.id.clone()) {
            return;
        }
        self.counted.push(counting::Record { at: chunk.at, id: chunk.id, usage: Usage::Prompt });
        self.prompted = self.prompted.max(Some(chunk.at));
    }

    fn clear_prompts(&mut self) {
        self.pending_chunks.clear();
        self.waiting_humans = 0;
        self.seen_prompts.clear();
        self.prompted = None;
    }

    fn reply(&self, body: &Value, at: SystemTime) -> Reply {
        let usage = &body["usage"];
        let input_total = usage["inputTokens"].as_u64().unwrap_or(0);
        let cached = usage["cachedReadTokens"].as_u64().unwrap_or(0);
        let turn = self.turns.iter().rev().find(|turn| turn.at <= at);
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

    fn note_model(&mut self) {
        let summary = json_file(&self.path.join("summary.json"));
        let signals = json_file(&self.path.join("signals.json"));
        let model = summary["current_model_id"]
            .as_str()
            .map(str::to_string)
            .or_else(|| self.turns.iter().rev().find_map(|turn| turn.model.clone()));
        let window = signals["contextWindowTokens"]
            .as_u64()
            .filter(|n| *n > 0)
            .or_else(|| summary["context_window"].as_u64())
            .unwrap_or(0);
        if let Some(model) = model.clone()
            && self.seen_model.as_ref().is_some_and(|seen| seen != &model || self.seen_window != window)
        {
            self.boundary = Some(SystemTime::now());
        }
        if model.is_some() {
            self.seen_model.clone_from(&model);
            self.seen_window = window;
        }
        self.model = model;
        self.window = window;
        self.signals_used = signals["contextTokensUsed"].as_u64().unwrap_or(0);
        self.signals_model = signals["primaryModelId"].as_str().map(str::to_string);
    }

    fn occupancy(&self) -> Option<u16> {
        if self.window == 0 {
            return None;
        }
        let model = self.model.as_deref()?;
        if let Some(reply) = &self.newest {
            let fresh = self.compact.is_none_or(|cut| reply.at > cut);
            let after = self.boundary.is_none_or(|cut| reply.at > cut);
            let same = reply.model.as_deref().is_none_or(|id| id == model);
            let one = reply.calls.is_some_and(|calls| calls <= 1) && reply.input_total <= self.window;
            return (fresh && after && same && one).then(|| context::percent(reply.input_total, self.window));
        }
        if self.boundary.is_some() || self.compact.is_some() {
            return None;
        }
        if self.signals_model.as_deref().is_some_and(|id| id != model) {
            return None;
        }
        (self.signals_used > 0 && self.signals_used <= self.window)
            .then(|| context::percent(self.signals_used, self.window))
    }

    fn activity_of(&self) -> Activity {
        if !self.turn_open {
            Activity::Idle
        } else if self.phase == "permission_prompt" || self.pending.values().any(|left| *left > 0) || self.asks > 0 {
            Activity::Waiting
        } else {
            Activity::Working
        }
    }

    fn recover_prompt(&mut self) {
        let Some(chunk) = newest_chunk(&self.path.join("updates.jsonl"), self.primary) else { return };
        if !newest_human(&self.path.join("chat_history.jsonl")) {
            return;
        }
        self.pending_chunks.retain(|item| item.id != chunk.id);
        self.emit_prompt(chunk);
        self.resumable = true;
    }

    fn recover_resumable(&mut self) {
        if self.resumable {
            return;
        }
        let Some(lines) = message::lines(&self.path.join("chat_history.jsonl")).ok().flatten() else { return };
        for line in lines {
            let Ok(line) = line else { continue };
            let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
            if entry["synthetic_reason"].as_str() == Some("compaction_meta")
                || (kind_of(&entry) == Some("assistant") && entry["content"].is_string())
                || (kind_of(&entry) == Some("user")
                    && entry.get("synthetic_reason").is_none()
                    && entry["prompt_index"].as_u64().is_some())
            {
                self.resumable = true;
                return;
            }
        }
    }

    fn read_children(&mut self) {
        let Ok(entries) = std::fs::read_dir(&self.path) else { return };
        let mut dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.join("updates.jsonl").is_file())
            .collect();
        dirs.sort();
        let mut next = Vec::new();
        for path in dirs {
            let mut child = self.take_child(&path);
            let updates = path.join("updates.jsonl");
            if let Some(batch) = read_tail(&mut child.tail, &updates) {
                for update in lines_of(&batch.bytes) {
                    if let Some(record) = child_token(&update) {
                        self.push_extra(path.to_string_lossy().into_owned(), record);
                    }
                }
            }
            next.push(child);
        }
        self.children = next;
    }

    fn take_child(&mut self, path: &Path) -> Child {
        self.children.iter().position(|child| child.path == path).map_or_else(
            || Child { path: path.to_path_buf(), tail: Tail::default() },
            |at| self.children.swap_remove(at),
        )
    }

    fn push_extra(&mut self, id: String, record: counting::Record) {
        if let Some((_, records)) = self.extras.iter_mut().find(|(key, _)| key == &id) {
            records.push(record);
        } else {
            self.extras.push((id, vec![record]));
        }
    }
}

struct Life {
    open: bool,
    primary: bool,
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

fn read_tail(tail: &mut Tail, path: &Path) -> Option<jsonl::Batch> {
    tail.read(path).ok()
}

fn lines_of(bytes: &[u8]) -> Vec<Value> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

fn exceeds(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|info| info.len() > TAIL_BYTES)
}

fn newest_life(path: &Path) -> Option<Life> {
    let lines = message::lines(path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        match entry["type"].as_str() {
            Some("turn_started") => return Some(Life { open: true, primary: is_primary(&entry) }),
            Some("turn_ended") => return Some(Life { open: false, primary: true }),
            _ => {}
        }
    }
    None
}

fn newest_human(path: &Path) -> bool {
    let Some(lines) = message::lines(path).ok().flatten() else { return false };
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if kind_of(&entry) == Some("user")
            && entry.get("synthetic_reason").is_none()
            && entry["prompt_index"].as_u64().is_some()
        {
            return true;
        }
    }
    false
}

fn newest_chunk(path: &Path, primary: bool) -> Option<Chunk> {
    let lines = message::lines(path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        let body = &entry["params"]["update"];
        if body["sessionUpdate"].as_str() != Some("user_message_chunk") {
            continue;
        }
        let text = body["content"]["text"].as_str().unwrap_or_default();
        if reminder(text) || body["_meta"]["promptIndex"].as_u64().is_none() {
            continue;
        }
        return chunk_of(&entry, primary);
    }
    None
}

fn chunk_of(update: &Value, primary: bool) -> Option<Chunk> {
    let body = &update["params"]["update"];
    let index = body["_meta"]["promptIndex"].as_u64()?;
    let at = millis(update).or_else(|| unix_time(&update["timestamp"]))?;
    let id = event_id(update).unwrap_or_else(|| fallback_id(update, index));
    Some(Chunk { at, id, primary })
}

fn child_token(update: &Value) -> Option<counting::Record> {
    let body = &update["params"]["update"];
    if body["sessionUpdate"].as_str() != Some("turn_completed") {
        return None;
    }
    let at = unix_time(&update["timestamp"])?;
    let usage = &body["usage"];
    let input_total = usage["inputTokens"].as_u64().unwrap_or(0);
    let cached = usage["cachedReadTokens"].as_u64().unwrap_or(0);
    let tokens = Tokens {
        input: input_total.saturating_sub(cached),
        out: usage["outputTokens"].as_u64().unwrap_or(0),
        cache_read: cached,
        cache_write: usage["cacheCreationTokens"].as_u64().unwrap_or(0),
    };
    if tokens.is_empty() {
        return None;
    }
    let id = body["prompt_id"].as_str().map_or_else(|| Stamp(at).to_string(), str::to_string);
    Some(counting::Record { at, id, usage: Usage::Completed(tokens) })
}

fn token_record(reply: &Reply) -> counting::Record {
    counting::Record { at: reply.at, id: reply.id.clone(), usage: Usage::Completed(reply.tokens) }
}

fn written(dir: &Path, text: &str) -> Option<String> {
    let path = dir.join("updates.jsonl");
    let lines = message::lines(&path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        let body = &entry["params"]["update"];
        if body["sessionUpdate"].as_str() != Some("agent_message_chunk") {
            continue;
        }
        if body["content"]["text"].as_str().map(str::trim) != Some(text) {
            continue;
        }
        return millis(&entry).or_else(|| unix_time(&entry["timestamp"])).map(|at| Stamp(at).to_string());
    }
    None
}

fn json_file(path: &Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null)
}

fn kind_of(entry: &Value) -> Option<&str> {
    entry["type"].as_str().or_else(|| entry["role"].as_str())
}

fn is_primary(event: &Value) -> bool {
    event["session_relationship"].as_str().is_none_or(|kind| kind == "primary")
}

fn asks_you(event: &Value) -> bool {
    event["tool_name"].as_str().is_some_and(|name| ASKS.contains(&name))
}

fn tool_name(event: &Value) -> String {
    event["tool_name"].as_str().unwrap_or_default().to_string()
}

fn reminder(text: &str) -> bool {
    text.contains("<system-reminder>") || text.contains("<user_info>")
}

fn event_id(update: &Value) -> Option<String> {
    update["params"]["_meta"]["eventId"]
        .as_str()
        .or_else(|| update["params"]["update"]["_meta"]["eventId"].as_str())
        .map(str::to_string)
}

fn fallback_id(update: &Value, index: u64) -> String {
    let stamp =
        update["params"]["_meta"]["agentTimestampMs"].as_u64().or_else(|| update["timestamp"].as_u64()).unwrap_or(0);
    format!("prompt-{stamp}-{index}")
}

fn millis(update: &Value) -> Option<SystemTime> {
    let ms = update["params"]["_meta"]["agentTimestampMs"]
        .as_u64()
        .or_else(|| update["params"]["update"]["_meta"]["agentTimestampMs"].as_u64())?;
    UNIX_EPOCH.checked_add(Duration::from_millis(ms))
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
