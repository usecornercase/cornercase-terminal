use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::counting::{self, Record, Usage};
use super::discovery::{self, folder};
use super::jsonl::{Tail, head};
use super::{Context, percent};
use crate::log::Stamp;
use crate::process;

pub(super) fn rollout_path(pid: i32, since: SystemTime) -> Option<PathBuf> {
    let mut pids = vec![pid];
    let mut paths = Vec::new();
    let mut at = 0;
    while at < pids.len() && at < 16 {
        let pid = pids[at];
        at += 1;
        if let Some(home) = home(pid) {
            paths.extend(rollouts(pid, &home));
        }
        for child in process::children(pid) {
            let args = process::args(child);
            let codex = args.iter().take(2).any(|arg| is_codex(arg) || arg.ends_with("/codex/bin/codex.js"));
            if codex && !pids.contains(&child) {
                pids.push(child);
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return served(pid, since);
    }
    single(paths.into_iter().filter(|p| metadata(p).is_some()))
}

fn served(pid: i32, since: SystemTime) -> Option<PathBuf> {
    let codex_home = home(pid)?;
    let cwd = folder(pid)?;
    let daemon = daemon(&codex_home)?;
    let mut paths = rollouts(daemon, &codex_home);
    paths.sort();
    paths.dedup();
    let path = single(paths.into_iter().filter(|path| {
        std::fs::metadata(path).and_then(|m| m.modified()).is_ok_and(|modified| modified >= since)
            && metadata(path).is_some_and(|(_, dir)| dir.and_then(|d| d.canonicalize().ok()).as_ref() == Some(&cwd))
    }))?;
    let agents = process::all().into_iter().filter(|&other| {
        let args = process::args(other);
        args.first().is_some_and(|arg| is_codex(arg))
            && !args.iter().any(|arg| arg == "app-server")
            && folder(other).as_ref() == Some(&cwd)
            && home(other).as_ref() == Some(&codex_home)
    });
    single(agents).map(|_| path)
}

fn daemon(home: &Path) -> Option<i32> {
    let text = std::fs::read_to_string(home.join("app-server-daemon/daemon.pid")).ok()?;
    let pid = i32::try_from(serde_json::from_str::<Value>(&text).ok()?.get("pid")?.as_i64()?).ok()?;
    let args = process::args(pid);
    (args.iter().take(2).any(|arg| is_codex(arg)) && args.iter().any(|arg| arg == "app-server")).then_some(pid)
}

fn rollouts(pid: i32, home: &Path) -> Vec<PathBuf> {
    let sessions = home.join("sessions");
    process::open_files(pid)
        .into_iter()
        .filter(|path| path.starts_with(&sessions) && session_id(path).is_some())
        .collect()
}

fn single<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let item = items.next()?;
    items.next().is_none().then_some(item)
}

fn is_codex(arg: &str) -> bool {
    Path::new(arg).file_name().is_some_and(|name| name == "codex")
}

fn home(pid: i32) -> Option<PathBuf> {
    discovery::home(pid, "CODEX_HOME", "", ".codex")
}

fn session_id(path: &Path) -> Option<&str> {
    let name = path.file_name()?.to_str()?.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    let ids = name.get(20..)?;
    let id = ids.split('_').next()?;
    (id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')).then_some(id)
}

fn metadata(path: &Path) -> Option<(String, Option<PathBuf>)> {
    let entry = head(path)?;
    let payload = entry.get("payload")?;
    let id = payload.get("id")?.as_str()?;
    (entry.get("type")?.as_str()? == "session_meta"
        && matches!(payload.get("source")?.as_str(), Some("cli" | "exec" | "vscode"))
        && session_id(path) == Some(id))
    .then(|| (id.to_string(), payload.get("cwd").and_then(Value::as_str).map(PathBuf::from)))
}

#[derive(Debug)]
pub(super) struct Rollout {
    pub path: PathBuf,
    tail: Tail,
    id: Option<String>,
    model: Option<String>,
    percent: Option<u16>,
    turn: bool,
    prompted: Option<SystemTime>,
    pub counted: Vec<Record>,
}

impl Rollout {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            tail: Tail::default(),
            id: None,
            model: None,
            percent: None,
            turn: false,
            prompted: None,
            counted: Vec::new(),
        }
    }

    pub fn context(&self) -> Option<Context> {
        Some(Context { model: self.model.clone()?, percent: self.percent })
    }

    pub fn turn(&self) -> bool {
        self.turn
    }

    pub fn prompted(&self) -> Option<SystemTime> {
        self.prompted
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn update(&mut self) {
        if self.read_file().is_none() {
            *self = Self::new(std::mem::take(&mut self.path));
        }
    }

    fn read_file(&mut self) -> Option<()> {
        let batch = self.tail.read(&self.path).ok()?;
        if batch.reset {
            let tail = std::mem::take(&mut self.tail);
            *self = Self::new(self.path.clone());
            self.tail = tail;
        }
        if self.id.is_none() {
            self.id = Some(metadata(&self.path)?.0);
        }
        if batch.skipped {
            self.model = None;
            self.percent = None;
        }
        for line in batch.bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            self.apply(line);
        }
        Some(())
    }

    fn set_model(&mut self, model: Option<&Value>) {
        let model = model.and_then(Value::as_str).filter(|m| !m.trim().is_empty()).map(str::to_string);
        if model != self.model {
            self.model = model;
            self.percent = None;
        }
    }

    fn apply(&mut self, line: &[u8]) {
        let Ok(entry) = serde_json::from_slice::<Value>(line) else {
            self.percent = None;
            return;
        };
        if let Some(record) = counted(&entry) {
            self.counted.push(record);
        }
        let payload = &entry["payload"];
        match entry["type"].as_str() {
            Some("turn_context") => self.set_model(payload.get("model")),
            Some("compacted") => self.percent = None,
            Some("event_msg") => match payload["type"].as_str() {
                Some("thread_settings_applied") => {
                    if payload.get("thread_id").is_none_or(|id| id.as_str() == self.id.as_deref()) {
                        self.set_model(payload["thread_settings"].get("model"));
                    }
                }
                Some("token_count") => {
                    if payload.get("info").is_some_and(Value::is_null) {
                        return;
                    }
                    let info = &payload["info"];
                    self.percent = info["last_token_usage"]["total_tokens"]
                        .as_u64()
                        .zip(info["model_context_window"].as_u64().filter(|window| *window > 0))
                        .map(|(tokens, window)| percent(tokens, window));
                }
                Some("context_compacted") => self.percent = None,
                Some("task_started") => self.turn = true,
                Some("task_complete" | "turn_aborted") => self.turn = false,
                Some("item_completed") if payload["item"]["type"] == "UserMessage" => {
                    let at = entry["timestamp"].as_str().and_then(Stamp::parse).unwrap_or_else(SystemTime::now);
                    self.prompted = self.prompted.max(Some(at));
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn counted(entry: &Value) -> Option<Record> {
    if entry["type"] != "event_msg" {
        return None;
    }
    let at = counting::timestamp(entry)?;
    let payload = &entry["payload"];
    let usage = match payload["type"].as_str()? {
        "item_completed" if payload["item"]["type"] == "UserMessage" => Usage::Prompt,
        "token_count" => {
            let info = payload.get("info")?.as_object()?;
            let convert = |v: &Value| {
                let mut tokens = counting::tokens(v, "input_tokens", "output_tokens", "cached_input_tokens", "unused");
                tokens.input = tokens.input.saturating_sub(tokens.cache_read);
                tokens
            };
            Usage::Cumulative {
                total: convert(info.get("total_token_usage")?),
                last: convert(info.get("last_token_usage")?),
            }
        }
        _ => return None,
    };
    Some(Record { at, id: payload["item"]["id"].as_str().map_or_else(|| format!("{at:?}"), str::to_string), usage })
}

#[cfg(test)]
mod tests;
