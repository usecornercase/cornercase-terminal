use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::agents::context::counting::{self, Record, Usage};
use crate::agents::context::discovery::{folder, home, named};
use crate::agents::context::jsonl::{Tail, head};
use crate::agents::context::message::{self, Backwards, Each, Role, Said, Spoken};
use crate::agents::context::{Context, percent};
use crate::error::{Error, Result};
use crate::process;
use crate::time::Stamp;

const NAME: &str = "gemini";
const REPLIES: usize = 512;

#[derive(Debug)]
struct Reply {
    id: String,
    model: Option<String>,
    input: Option<u64>,
    written: Option<SystemTime>,
}

#[derive(Debug)]
pub(in crate::agents) struct Session {
    pub(crate) path: PathBuf,
    id: String,
    tail: Tail,
    replies: Vec<Reply>,
    footer: Option<String>,
    picked: Option<(String, SystemTime)>,
    pub(crate) context: Option<Context>,
    pub(crate) prompted: Option<SystemTime>,
    used: bool,
    model: Option<String>,
    pub(crate) counted: Vec<Record>,
}

impl Session {
    fn new(path: PathBuf, id: String) -> Self {
        Self {
            path,
            id,
            tail: Tail::default(),
            replies: Vec::new(),
            footer: None,
            picked: None,
            context: None,
            prompted: None,
            used: false,
            model: None,
            counted: Vec::new(),
        }
    }

    pub(in crate::agents) fn id(&self) -> Option<&str> {
        self.used.then_some(&self.id)
    }

    fn update(&mut self, footer: Option<String>) -> Option<()> {
        let batch = self.tail.read(&self.path).ok()?;
        if batch.reset {
            self.replies.clear();
            self.context = None;
            self.prompted = None;
            self.used = false;
            self.model = None;
            self.footer = None;
            self.picked = None;
            self.id = metadata(&self.path)?;
        }
        if batch.skipped {
            self.replies.clear();
            self.context = None;
        }
        for line in batch.bytes.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let Ok(entry) = serde_json::from_slice::<Value>(line) else {
                self.replies.clear();
                continue;
            };
            self.apply(&entry);
        }
        if footer.is_some() && footer != self.footer {
            if self.footer.is_some() || footer.as_deref() != Some("auto") {
                self.picked = footer.clone().map(|model| (model, SystemTime::now()));
            }
            self.footer = footer;
        }
        self.context = self.context();
        Some(())
    }

    fn apply(&mut self, entry: &Value) {
        let Some(set) = entry.get("$set") else {
            self.message(entry);
            return;
        };
        if let Some(messages) = set.get("messages") {
            self.replies.clear();
            for message in messages.as_array().into_iter().flatten() {
                self.message(message);
            }
        }
    }

    fn message(&mut self, entry: &Value) {
        if let Some(record) = counted(entry) {
            self.counted.push(record);
        }
        let Some(id) = entry["id"].as_str() else { return };
        if prompt(entry) {
            self.prompted = self.prompted.max(entry["timestamp"].as_str().and_then(Stamp::parse));
            self.used = true;
        }
        if entry["type"] == "info"
            && let Some(model) = entry["content"].as_str().and_then(|t| t.strip_prefix("Model set to "))
        {
            let model = model.strip_suffix(" (persisted)").unwrap_or(model);
            self.picked = Some((
                model.into(),
                entry["timestamp"].as_str().and_then(Stamp::parse).unwrap_or_else(SystemTime::now),
            ));
        }
        if entry["type"] != "gemini" {
            return;
        }
        self.used = true;
        let reply = Reply {
            id: id.into(),
            model: entry["model"].as_str().filter(|m| !m.trim().is_empty()).map(str::to_string),
            input: entry["tokens"]["input"].as_u64(),
            written: entry["timestamp"].as_str().and_then(Stamp::parse),
        };
        if reply.model.is_some() {
            self.model.clone_from(&reply.model);
        }
        if let Some(at) = self.replies.iter().position(|r| r.id == reply.id) {
            self.replies[at] = reply;
        } else {
            self.replies.push(reply);
            if self.replies.len() > REPLIES {
                self.replies.remove(0);
            }
        }
    }

    fn context(&self) -> Option<Context> {
        let reply = self.replies.iter().rev().find(|r| r.input.is_some());
        let model = reply.and_then(|r| r.model.as_deref());
        let changed = self.picked.as_ref().filter(|(picked, at)| {
            Some(picked.as_str()) != model && reply.and_then(|r| r.written).is_none_or(|written| written < *at)
        });
        if let Some((model, _)) = changed {
            return Some(Context { model: model.clone(), percent: None });
        }
        let Some(model) = model else {
            return self.model.as_ref().map(|model| Context { model: model.clone(), percent: None });
        };
        let window = if model.starts_with("gemma-4") { 256_000 } else { 1_048_576 };
        Some(Context { model: model.into(), percent: reply.and_then(|r| r.input).map(|n| percent(n, window)) })
    }
}

fn counted(entry: &Value) -> Option<Record> {
    let at = counting::timestamp(entry)?;
    let id = entry["id"].as_str()?.into();
    let usage = if prompt(entry) {
        Usage::Prompt
    } else if entry["type"] == "gemini" && entry["tokens"].is_object() {
        let value = &entry["tokens"];
        let mut tokens = counting::tokens(value, "input", "output", "cached", "unused");
        tokens.input = tokens.input.saturating_sub(tokens.cache_read);
        tokens.out = tokens.out.saturating_add(value["thoughts"].as_u64().unwrap_or(0));
        Usage::Tokens(tokens)
    } else {
        return None;
    };
    Some(Record { at, id, usage })
}

pub(in crate::agents) fn look(
    pid: i32,
    since: SystemTime,
    previous: Option<Session>,
    footer: Option<String>,
) -> Option<Session> {
    look_among(pid, since, previous, footer, process::all)
}

fn look_among(
    pid: i32,
    since: SystemTime,
    previous: Option<Session>,
    footer: Option<String>,
    scan: impl FnOnce() -> Vec<i32>,
) -> Option<Session> {
    let others = scan();
    let family: Vec<i32> = std::iter::once(pid).chain(process::descendants(pid)).collect();
    let holder = family.iter().rev().copied().find(|&pid| named(&process::args(pid), NAME)).unwrap_or(pid);
    let dir = home(holder, "GEMINI_CLI_HOME", ".gemini", ".gemini")?;
    let cwd = folder(holder)?;
    if taken_by_another(others, &family, pid, &cwd, &dir) {
        return None;
    }
    let projects: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("projects.json")).ok()?).ok()?;
    let slug = projects["projects"].as_object()?.iter().find_map(|(path, slug)| {
        (Path::new(path).canonicalize().ok().as_ref() == Some(&cwd)).then(|| slug.as_str()).flatten()
    })?;
    if slug.is_empty() || Path::new(slug).components().count() != 1 || slug == "." || slug == ".." {
        return None;
    }
    let project = dir.join("tmp").join(slug);
    if let Ok(owner) = std::fs::read_to_string(project.join(".project_root"))
        && Path::new(owner.trim()).canonicalize().ok().as_ref() != Some(&cwd)
    {
        return None;
    }
    let (path, id) = newest_chat(&project.join("chats"), since)?;
    let mut session = previous.filter(|s| s.path == path && s.id == id).unwrap_or_else(|| Session::new(path, id));
    session.update(footer)?;
    Some(session)
}

fn taken_by_another(others: Vec<i32>, family: &[i32], pid: i32, cwd: &Path, dir: &Path) -> bool {
    others.into_iter().any(|other| {
        !family.contains(&other)
            && named(&process::args(other), NAME)
            && folder(other).as_deref() == Some(cwd)
            && home(other, "GEMINI_CLI_HOME", ".gemini", ".gemini").as_deref() == Some(dir)
            && !process::descendants(other).contains(&pid)
    })
}

fn newest_chat(chats: &Path, since: SystemTime) -> Option<(PathBuf, String)> {
    std::fs::read_dir(chats)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "jsonl") || !entry.file_type().ok()?.is_file() {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            (modified >= since).then(|| Some((modified, path.clone(), metadata(&path)?))).flatten()
        })
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
        .map(|(_, path, id)| (path, id))
}

fn metadata(path: &Path) -> Option<String> {
    let entry = head(path)?;
    let id = entry["sessionId"].as_str()?;
    (entry["kind"] == "main" && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then(|| id.into())
}

fn text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => message::joined([text.clone()]),
        Value::Array(parts) => message::joined(
            parts.iter().filter(|p| p["thought"] != true).filter_map(|p| p["text"].as_str().map(str::to_string)),
        ),
        _ => None,
    }
}

fn prompt(entry: &Value) -> bool {
    entry["type"] == "user"
        && !entry["content"].as_array().is_some_and(|parts| parts.iter().any(|p| p.get("functionResponse").is_some()))
        && text(&entry["content"]).is_some_and(|t| {
            let text = t.trim();
            !text.is_empty()
                && !["/", "?", "<session_context>", "<hook_context>", "<state_snapshot>", "<scratchpad>"]
                    .iter()
                    .any(|prefix| text.starts_with(prefix))
        })
}

pub(in crate::agents) fn last_message(path: &Path) -> Result<Option<Said>> {
    let mut found = None;
    said_all(path, &mut |spoken| {
        found = (spoken.role == Role::Reply).then_some(spoken.said);
        found.is_none()
    })?;
    Ok(found)
}

pub(in crate::agents) fn said_all(path: &Path, each: Each) -> Result<()> {
    let failed = |e: std::io::Error| Error::Record { path: path.to_path_buf(), cause: e.to_string() };
    let lines = match Backwards::open(path) {
        Ok(lines) => lines,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    let mut seen = HashSet::new();
    for line in lines {
        let line = line.map_err(failed)?;
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if let Some(messages) = entry["$set"].get("messages") {
            spoken_messages(messages, &mut seen, each);
            return Ok(());
        }
        if !spoken(&entry, &mut seen, each) {
            return Ok(());
        }
    }
    Ok(())
}

fn spoken_messages(messages: &Value, seen: &mut HashSet<String>, each: Each) {
    for entry in messages.as_array().into_iter().flatten().rev() {
        if !spoken(entry, seen, each) {
            return;
        }
    }
}

fn spoken(entry: &Value, seen: &mut HashSet<String>, each: Each) -> bool {
    if entry["type"] == "user" {
        if prompt(entry)
            && let Some(text) = text(&entry["content"])
            && let Some(id) = entry["id"].as_str()
            && seen.insert(id.to_string())
        {
            let said = Said { text, written: entry["timestamp"].as_str().map(str::to_string) };
            return each(Spoken { role: Role::Prompt, said });
        }
        return true;
    }
    said(entry, seen).is_none_or(|said| each(Spoken { role: Role::Reply, said }))
}

fn said(entry: &Value, seen: &mut HashSet<String>) -> Option<Said> {
    if !seen.insert(entry["id"].as_str()?.to_string()) || entry["type"] != "gemini" || entry["model"].as_str().is_none()
    {
        return None;
    }
    Some(Said { text: text(&entry["content"])?, written: entry["timestamp"].as_str().map(str::to_string) })
}

pub(in crate::agents) fn footer_model(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen.lines().rev().take(6).collect();
    for rows in lines.windows(2) {
        let header = footer_cells(rows[1]);
        let Some(at) = header.iter().position(|cell| *cell == "/model") else { continue };
        let values = footer_cells(rows[0]);
        if header.len() != values.len() {
            continue;
        }
        let model = values[at];
        if model == "Auto" {
            return Some("auto".into());
        }
        return (model.starts_with("gemini-") || model.starts_with("gemma-"))
            .then_some(model)
            .filter(|model| {
                !model.contains("...") && model.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
            })
            .map(str::to_string);
    }
    None
}

fn footer_cells(line: &str) -> Vec<&str> {
    line.split("  ").map(str::trim).filter(|cell| !cell.is_empty()).collect()
}

#[cfg(test)]
mod tests;
