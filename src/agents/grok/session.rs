use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::agents::activity::Activity;
use crate::agents::context::counting::{self, Usage};
use crate::agents::context::jsonl::{self, Tail};
use crate::agents::context::message::{self, Each, Role, Said, Spoken};
use crate::agents::context::{self, Compaction, Context};
use crate::agents::stats::Tokens;
use crate::error::Result;
use crate::process;
use crate::time::Stamp;

const ASKS: [&str; 2] = ["ask_user_question", "exit_plan_mode"];
const TURNS: usize = 64;
const PROMPT_IDS: usize = 32;
const BACK_DEFAULT: u64 = 64 * 1024 * 1024;

thread_local! {
    static BACK_LIMIT: Cell<u64> = const { Cell::new(BACK_DEFAULT) };
}

#[derive(Debug)]
pub(in crate::agents) struct Session {
    pub(in crate::agents) path: PathBuf,
    pub(in crate::agents) id: String,
    pub(in crate::agents) activity: Activity,
    pub(in crate::agents) turn_open: bool,
    pub(in crate::agents) context: Option<Context>,
    pub(in crate::agents) compaction: Compaction,
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
    signals_at: Option<SystemTime>,
    prompt_at: Option<SystemTime>,
    prompt_ids: HashSet<String>,
    newest_event: Option<SystemTime>,
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
    end: Option<SystemTime>,
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

fn query(content: &Value) -> Option<String> {
    let text = message::joined(message::texts(Some(content), "text"))?;
    let text = text.strip_prefix("<user_query>").and_then(|text| text.strip_suffix("</user_query>")).unwrap_or(&text);
    message::joined([text.to_string()])
}

pub(in crate::agents) fn said_all(dir: &Path, each: Each) -> Result<()> {
    let chat = dir.join("chat_history.jsonl");
    let Some(lines) = message::lines(&chat)? else { return Ok(()) };
    for line in lines {
        let line = line.map_err(|e| message::unreadable(&chat, &e))?;
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        let spoken = match kind_of(&entry) {
            Some("assistant") => entry["content"]
                .as_str()
                .and_then(|text| message::joined([text.to_string()]))
                .map(|text| (Role::Reply, text)),
            Some("user") if entry.get("synthetic_reason").is_none() && entry.get("prompt_index").is_some() => {
                query(&entry["content"]).map(|text| (Role::Prompt, text))
            }
            _ => None,
        };
        if let Some((role, text)) = spoken
            && !each(Spoken { role, said: Said { text, written: None } })
        {
            return Ok(());
        }
    }
    Ok(())
}

impl Session {
    fn open(path: PathBuf, id: String) -> Self {
        Self {
            path,
            id,
            activity: Activity::Idle,
            turn_open: false,
            context: None,
            compaction: Compaction::default(),
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
            signals_at: None,
            prompt_at: None,
            prompt_ids: HashSet::new(),
            newest_event: None,
            children: Vec::new(),
        }
    }

    fn refresh(&mut self) {
        let opening = self.start == Start::New;
        self.start = Start::Seen;
        self.read_events(opening);
        let replayed = self.read_updates();
        self.read_chat();
        if opening || replayed {
            self.recover_prompt();
        }
        if opening {
            self.recover_resumable();
        }
        self.note_model();
        let percent = self.occupancy();
        self.context = self.model.clone().map(|model| Context { model, percent });
        self.compaction = self.compaction_state(percent);
        self.activity = self.activity_of();
        self.read_children();
    }

    fn read_events(&mut self, opening: bool) {
        let path = self.path.join("events.jsonl");
        let Some(batch) = read_tail(&mut self.events, &path) else { return };
        let mut unknown = false;
        if opening || batch.reset {
            self.turn_open = false;
            self.phase.clear();
            self.pending.clear();
            self.asks = 0;
            self.turns.clear();
            self.primary = true;
            self.newest_event = None;
            match newest_life(&path) {
                Found::Open(primary) => {
                    self.turn_open = true;
                    self.primary = primary;
                }
                Found::Closed => self.turn_open = false,
                Found::Unknown => unknown = true,
            }
        }
        let mut saw_work = false;
        let mut closed = false;
        for event in jsonl::values(&batch.bytes) {
            if work_event(&event) {
                saw_work = true;
            }
            if event["type"].as_str() == Some("turn_ended") {
                closed = true;
            }
            self.apply_event(&event);
        }
        if unknown && saw_work && !closed {
            self.turn_open = true;
        }
    }

    fn read_updates(&mut self) -> bool {
        let path = self.path.join("updates.jsonl");
        let Some(batch) = read_tail(&mut self.updates, &path) else { return false };
        if batch.reset {
            self.compact = None;
            self.newest = None;
        }
        for update in jsonl::values(&batch.bytes) {
            self.apply_update(&update);
        }
        batch.reset
    }

    fn read_chat(&mut self) {
        let path = self.path.join("chat_history.jsonl");
        let Some(batch) = read_tail(&mut self.chat, &path) else { return };
        if batch.reset {
            self.resumable = false;
        }
        for line in jsonl::values(&batch.bytes) {
            self.apply_chat(&line);
        }
    }

    fn apply_event(&mut self, event: &Value) {
        match event["type"].as_str() {
            Some("turn_started") => self.turn_started(event),
            Some("turn_ended") => self.turn_ended(event),
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

    fn reset_turn(&mut self, open: bool) {
        self.turn_open = open;
        self.phase.clear();
        self.pending.clear();
        self.asks = 0;
    }

    fn turn_started(&mut self, event: &Value) {
        self.reset_turn(true);
        self.primary = is_primary(event);
        if let Some(at) = event_time(event) {
            self.note_event(at);
            self.turns.push(Turn {
                at,
                end: None,
                model: event["model_id"].as_str().map(str::to_string),
                primary: self.primary,
            });
            if self.turns.len() > TURNS {
                self.turns.remove(0);
            }
        }
    }

    fn turn_ended(&mut self, event: &Value) {
        self.reset_turn(false);
        self.primary = true;
        if let Some(at) = event_time(event) {
            self.note_event(at);
            if let Some(turn) = self.turns.iter_mut().rev().find(|turn| turn.end.is_none()) {
                turn.end = Some(at);
            }
        }
    }

    fn apply_update(&mut self, update: &Value) {
        let body = &update["params"]["update"];
        let at = update_time(update);
        match body["sessionUpdate"].as_str() {
            Some("auto_compact_completed" | "compaction_checkpoint") => self.compact = self.compact.max(at),
            Some("turn_completed") => {
                if let Some(at) = at {
                    self.turn_completed(body, at);
                }
            }
            Some("user_message_chunk") => self.note_chunk(update),
            _ => {}
        }
    }

    fn turn_completed(&mut self, body: &Value, at: SystemTime) {
        let reply = self.reply(body, at);
        if !reply.tokens.is_empty() {
            let record = token_record(&reply);
            if reply.primary {
                self.counted.push(record);
            } else {
                self.push_extra(format!("{}/side", self.id), record);
            }
        }
        if reply.primary {
            self.newest = Some(reply);
        }
    }

    fn apply_chat(&mut self, line: &Value) {
        if line["synthetic_reason"].as_str() == Some("compaction_meta")
            || (kind_of(line) == Some("assistant") && line["content"].is_string())
            || (kind_of(line) == Some("user")
                && line.get("synthetic_reason").is_none()
                && line["prompt_index"].as_u64().is_some())
        {
            self.resumable = true;
        }
    }

    fn note_chunk(&mut self, update: &Value) {
        let Some(chunk) = human_chunk(update) else { return };
        if !self.primary_at(chunk.at) {
            return;
        }
        self.emit_prompt(chunk);
    }

    fn admits(&mut self, chunk: &Chunk) -> bool {
        let Some(latest) = self.prompt_at else { return true };
        if chunk.at < latest {
            return false;
        }
        if chunk.at > latest {
            self.prompt_ids.clear();
            return true;
        }
        !(self.prompt_ids.contains(&chunk.id) || self.prompt_ids.len() >= PROMPT_IDS)
    }

    fn emit_prompt(&mut self, chunk: Chunk) {
        if !self.admits(&chunk) {
            return;
        }
        self.prompt_ids.insert(chunk.id.clone());
        self.prompt_at = Some(chunk.at);
        self.counted.push(counting::Record { at: chunk.at, id: chunk.id, usage: Usage::Prompt });
        self.prompted = self.prompted.max(Some(chunk.at));
    }

    fn primary_at(&self, at: SystemTime) -> bool {
        self.turns
            .iter()
            .rev()
            .find(|turn| turn.at <= at && turn.end.is_none_or(|end| at <= end))
            .is_none_or(|turn| turn.primary)
    }

    fn note_event(&mut self, at: SystemTime) {
        self.newest_event = self.newest_event.map_or(Some(at), |latest| Some(latest.max(at)));
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
        let changed = model.as_ref().is_some_and(|model| {
            self.seen_model.as_ref().is_some_and(|seen| seen != model || self.seen_window != window)
        });
        if changed {
            let model_changed = self.seen_model.as_deref() != model.as_deref();
            if model_changed {
                match self.fresh_turn(model.as_deref()) {
                    Some(mark) => self.boundary = Some(self.boundary.map_or(mark, |kept| kept.max(mark))),
                    None => self.invalidate_selection(),
                }
            } else if let Some(mark) = self.transition(model.as_deref()) {
                self.boundary = Some(self.boundary.map_or(mark, |kept| kept.max(mark)));
            }
        }
        if model.is_some() {
            self.seen_model.clone_from(&model);
            self.seen_window = window;
        }
        self.model = model;
        self.window = window;
        self.signals_used = signals["contextTokensUsed"].as_u64().unwrap_or(0);
        self.signals_model = signals["primaryModelId"].as_str().map(str::to_string);
        self.signals_at = std::fs::metadata(self.path.join("signals.json")).and_then(|info| info.modified()).ok();
    }

    fn fresh_turn(&self, model: Option<&str>) -> Option<SystemTime> {
        let started = self
            .turns
            .iter()
            .rev()
            .find(|turn| model.is_some_and(|id| turn.model.as_deref() == Some(id)))
            .map(|turn| turn.at)?;
        self.boundary.is_none_or(|cut| started > cut).then_some(started)
    }

    fn invalidate_selection(&mut self) {
        let mut mark = self.newest.as_ref().map(|reply| reply.at);
        if let Some(at) = self.signals_at {
            mark = Some(mark.map_or(at, |kept| kept.max(at)));
        }
        let Some(mark) = mark else { return };
        let past = mark.checked_add(Duration::from_millis(1)).unwrap_or(mark);
        self.boundary = Some(self.boundary.map_or(past, |kept| kept.max(past)));
    }

    fn transition(&self, model: Option<&str>) -> Option<SystemTime> {
        let recorded = self
            .turns
            .iter()
            .rev()
            .find(|turn| model.is_some_and(|id| turn.model.as_deref() == Some(id)))
            .map(|turn| turn.at);
        recorded.or(self.newest_event)
    }

    fn compaction_state(&self, percent: Option<u16>) -> Compaction {
        let Some(cut) = self.compact else { return Compaction::default() };
        let before = |at: Option<SystemTime>| at.is_none_or(|at| at < cut);
        let hides = percent.is_none()
            && before(self.newest.as_ref().map(|reply| reply.at))
            && before(self.signals_at)
            && before(self.boundary);
        Compaction { at: Some(cut), hides }
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
            if fresh && after && same && one {
                return Some(context::percent(reply.input_total, self.window));
            }
        }
        self.signals_percent(model)
    }

    fn signals_percent(&self, model: &str) -> Option<u16> {
        let at = self.signals_at?;
        if self.signals_model.as_deref().is_some_and(|id| id != model) {
            return None;
        }
        if self.newest.is_some() && self.signals_model.as_deref() != Some(model) {
            return None;
        }
        if self.newest.as_ref().is_some_and(|reply| at < reply.at) {
            return None;
        }
        if self.compact.is_some_and(|cut| cut > at) || self.boundary.is_some_and(|cut| cut > at) {
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
        let Some(chunk) = newest_chunk(&self.path.join("updates.jsonl")) else { return };
        if !self.primary_at(chunk.at) {
            return;
        }
        self.emit_prompt(chunk);
    }

    #[cfg(test)]
    pub(super) fn retained_prompts(&self) -> usize {
        self.prompt_ids.len()
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
            self.read_child(&path, &mut child.tail);
            next.push(child);
        }
        self.children = next;
    }

    fn read_child(&mut self, path: &Path, tail: &mut Tail) {
        let Some(batch) = read_tail(tail, &path.join("updates.jsonl")) else { return };
        for update in jsonl::values(&batch.bytes) {
            if let Some(record) = child_token(&update) {
                self.push_extra(path.to_string_lossy().into_owned(), record);
            }
        }
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

enum Found {
    Open(bool),
    Closed,
    Unknown,
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

fn back_limit() -> u64 {
    BACK_LIMIT.with(Cell::get)
}

#[cfg(test)]
pub(in crate::agents) fn set_back_limit(limit: u64) {
    BACK_LIMIT.with(|cell| cell.set(limit));
}

fn newest_life(path: &Path) -> Found {
    let limit = back_limit();
    let big = std::fs::metadata(path).is_ok_and(|info| info.len() > limit);
    let Ok(Some(lines)) = message::lines_within(path, limit) else {
        return if big { Found::Unknown } else { Found::Closed };
    };
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        match entry["type"].as_str() {
            Some("turn_started") => return Found::Open(is_primary(&entry)),
            Some("turn_ended") => return Found::Closed,
            _ => {}
        }
    }
    if big { Found::Unknown } else { Found::Closed }
}

fn newest_chunk(path: &Path) -> Option<Chunk> {
    let lines = message::lines(path).ok()??;
    for line in lines {
        let Ok(line) = line else { continue };
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if let Some(chunk) = human_chunk(&entry) {
            return Some(chunk);
        }
    }
    None
}

fn human_chunk(update: &Value) -> Option<Chunk> {
    let body = &update["params"]["update"];
    if body["sessionUpdate"].as_str() != Some("user_message_chunk") {
        return None;
    }
    let index = body["_meta"]["promptIndex"].as_u64()?;
    match explicit_synthetic(update) {
        Some(true) => return None,
        Some(false) => {}
        None => {
            let text = body["content"]["text"].as_str().unwrap_or_default();
            if reminder_only(text) {
                return None;
            }
        }
    }
    let at = update_time(update)?;
    let ms = stamp_ms(update).unwrap_or(0);
    Some(Chunk { at, id: format!("{index}-{ms}") })
}

fn work_event(event: &Value) -> bool {
    matches!(
        event["type"].as_str(),
        Some(
            "phase_changed"
                | "tool_started"
                | "tool_completed"
                | "permission_requested"
                | "loop_started"
                | "first_token"
        )
    )
}

fn explicit_synthetic(update: &Value) -> Option<bool> {
    update["params"]["update"]["_meta"]["syntheticPrompt"]
        .as_bool()
        .or_else(|| update["params"]["_meta"]["syntheticPrompt"].as_bool())
}

fn reminder_only(text: &str) -> bool {
    let text = text.trim_start();
    (text.starts_with("<system-reminder>") || text.starts_with("<user_info>")) && !text.contains("<user_query>")
}

fn update_time(update: &Value) -> Option<SystemTime> {
    millis(update).or_else(|| unix_time(&update["timestamp"]))
}

fn stamp_ms(update: &Value) -> Option<u64> {
    update["params"]["_meta"]["agentTimestampMs"]
        .as_u64()
        .or_else(|| update["params"]["update"]["_meta"]["agentTimestampMs"].as_u64())
        .or_else(|| update["timestamp"].as_u64().map(|secs| secs.saturating_mul(1000)))
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
