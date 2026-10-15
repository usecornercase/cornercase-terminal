use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, params};
use serde_json::Value;

use super::proto::{self, Field};
use crate::agents::context::counting::{Record, Usage};
use crate::agents::context::discovery;
use crate::agents::context::jsonl::Tail;
use crate::agents::context::message::{self, Said};
use crate::agents::context::{Context, percent};
use crate::error::{self, Error};
use crate::process;
use crate::time::Stamp;

const BUSY_FOR: Duration = Duration::from_millis(100);
const AUTO: &str = "Auto";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Turn {
    prompt: Option<String>,
    user: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Session {
    pub(crate) path: PathBuf,
    pub(crate) id: String,
    pub(crate) context: Option<Context>,
    pub(crate) prompted: Option<SystemTime>,
    pub(crate) counted: Vec<Record>,
    pub(crate) turn_open: bool,
    pub(crate) resumable: bool,
    transcript: Option<PathBuf>,
    tail: Tail,
    root: Option<String>,
    root_model: Option<String>,
    cache: HashMap<String, Turn>,
    order: Vec<String>,
    tokens: Option<(u64, u64)>,
    known: HashSet<String>,
    read: bool,
}

struct Meta {
    root: Option<String>,
    model: Option<String>,
    created: Option<u64>,
}

pub(in crate::agents) fn follow(pid: i32, since: SystemTime, previous: Option<Session>) -> Option<Session> {
    let (path, id) = store_of(pid)?;
    let mut session = match previous {
        Some(session) if session.path == path => session,
        _ => Session { path, id, ..Session::default() },
    };
    session.prompted = None;
    session.counted.clear();
    if session.transcript.is_none() {
        session.transcript = transcript_of(pid, &session.path, &session.id);
    }
    session.refresh(since);
    session.follow_transcript();
    Some(session)
}

fn store_of(pid: i32) -> Option<(PathBuf, String)> {
    let mut found: Vec<(PathBuf, String)> = process::open_files(pid).iter().filter_map(|path| chat(path)).collect();
    found.sort();
    found.dedup();
    if found.len() == 1 { found.pop() } else { None }
}

fn chat(path: &Path) -> Option<(PathBuf, String)> {
    let mut parts = path.components().rev().filter_map(|c| c.as_os_str().to_str());
    let (file, id, bucket, folder) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let hex = bucket.len() == 32 && bucket.bytes().all(|b| b.is_ascii_hexdigit());
    let named = !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    (file == "store.db" && folder == "chats" && hex && named).then(|| (path.to_path_buf(), id.to_string()))
}

pub(in crate::agents) fn slug(path: &str) -> String {
    let mut out = String::new();
    for c in path.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !out.ends_with('-') {
            out.push(c);
        }
    }
    out.trim_matches('-').to_string()
}

fn transcript_of(pid: i32, store: &Path, id: &str) -> Option<PathBuf> {
    let meta: Value = serde_json::from_slice(&std::fs::read(store.parent()?.join("meta.json")).ok()?).ok()?;
    let cwd = meta["cwd"].as_str().filter(|cwd| !cwd.is_empty())?;
    let data = discovery::home(pid, "CURSOR_DATA_DIR", "", ".cursor")?;
    Some(data.join("projects").join(slug(cwd)).join("agent-transcripts").join(id).join(format!("{id}.jsonl")))
}

fn open(store: &Path) -> rusqlite::Result<Connection> {
    let connection =
        Connection::open_with_flags(store, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    connection.busy_timeout(BUSY_FOR)?;
    Ok(connection)
}

fn blob(connection: &Connection, id: &str) -> Option<Vec<u8>> {
    connection.query_row("SELECT data FROM blobs WHERE id = ?1", params![id], |row| row.get::<_, Vec<u8>>(0)).ok()
}

fn meta(connection: &Connection) -> Option<Meta> {
    let hex: String =
        connection.query_row("SELECT value FROM meta WHERE key = '0'", [], |row| row.get::<_, String>(0)).ok()?;
    let value: Value = serde_json::from_slice(&proto::from_hex(hex.trim())?).ok()?;
    let text = |key: &str| value[key].as_str().filter(|text| !text.trim().is_empty()).map(str::to_string);
    Some(Meta { root: text("latestRootBlobId"), model: text("lastUsedModel"), created: value["createdAt"].as_u64() })
}

fn blobs_of(fields: &[Field<'_>], number: u32) -> Option<Vec<String>> {
    proto::all_bytes(fields, number).into_iter().map(proto::blob_id).collect()
}

fn agent_turn(connection: &Connection, id: &str) -> Option<Vec<u8>> {
    let data = blob(connection, id)?;
    let fields = proto::fields(&data)?;
    proto::bytes(&fields, 1).map(<[u8]>::to_vec)
}

fn read_turn(connection: &Connection, id: &str) -> Option<Turn> {
    let data = blob(connection, id)?;
    let outer = proto::fields(&data)?;
    let Some(inner) = proto::bytes(&outer, 1) else {
        return Some(Turn { prompt: None, user: None, model: None });
    };
    let fields = proto::fields(inner)?;
    let user = proto::bytes(&fields, 1).and_then(proto::blob_id);
    let prompt = proto::text(&fields, 10).filter(|id| !id.is_empty()).or_else(|| {
        let message = blob(connection, user.as_deref()?)?;
        proto::text(&proto::fields(&message)?, 2).filter(|id| !id.is_empty())
    });
    Some(Turn { prompt, user, model: proto::text(&fields, 7).filter(|name| !name.trim().is_empty()) })
}

fn moment(millis: u64) -> Option<SystemTime> {
    (millis > 0).then(|| UNIX_EPOCH.checked_add(Duration::from_millis(millis))).flatten()
}

fn started_at(connection: &Connection, user: Option<&str>) -> Option<SystemTime> {
    let message = blob(connection, user?)?;
    moment(proto::number(&proto::fields(&message)?, 25)?)
}

impl Session {
    fn refresh(&mut self, since: SystemTime) {
        let Some(connection) = open(&self.path).ok() else { return self.forget() };
        let Some(meta) = meta(&connection) else { return self.forget() };
        let first = !self.read;
        self.read = true;
        if meta.root != self.root {
            self.root.clone_from(&meta.root);
            self.root_model.clone_from(&meta.model);
            self.load(&connection, &meta, since, first);
        }
        self.show(&meta);
    }

    fn forget(&mut self) {
        self.context = None;
        self.resumable = false;
        self.root = None;
    }

    fn load(&mut self, connection: &Connection, meta: &Meta, since: SystemTime, first: bool) {
        let root = meta.root.as_deref().and_then(|id| blob(connection, id)).and_then(|data| {
            let fields = proto::fields(&data)?;
            let tokens = proto::bytes(&fields, 5).and_then(proto::fields);
            let used = tokens.as_ref().and_then(|t| proto::number(t, 1));
            let window = tokens.as_ref().and_then(|t| proto::number(t, 2));
            Some((blobs_of(&fields, 8)?, used.zip(window)))
        });
        let Some((ids, tokens)) = root else {
            self.resumable = false;
            self.tokens = None;
            self.order.clear();
            self.cache.clear();
            return;
        };
        let previous = std::mem::take(&mut self.cache);
        let fresh = meta.created.and_then(moment).is_some_and(|created| created >= since);
        for id in &ids {
            let turn = previous.get(id).cloned().or_else(|| read_turn(connection, id));
            let Some(turn) = turn else { continue };
            if let Some(prompt) = &turn.prompt
                && self.known.insert(prompt.clone())
                && (!first || fresh)
            {
                let at = started_at(connection, turn.user.as_deref()).unwrap_or_else(SystemTime::now);
                self.counted.push(Record { at, id: prompt.clone(), usage: Usage::Prompt });
                self.prompted = self.prompted.max(Some(at));
            }
            self.cache.insert(id.clone(), turn);
        }
        self.order = ids;
        self.tokens = tokens;
        self.resumable = !self.order.is_empty();
    }

    fn show(&mut self, meta: &Meta) {
        if self.root.is_none() || !self.resumable && self.tokens.is_none() {
            self.context = None;
            return;
        }
        let unchanged = meta.model == self.root_model;
        let routed = self.order.last().and_then(|id| self.cache.get(id)).and_then(|turn| turn.model.clone());
        let name = if unchanged { routed.or_else(|| meta.model.clone()) } else { meta.model.clone() };
        self.context = name.map(|name| Context {
            model: if name == "default" { AUTO.to_string() } else { name },
            percent: self
                .tokens
                .filter(|(_, window)| *window > 0 && unchanged)
                .map(|(used, window)| percent(used, window)),
        });
    }

    fn follow_transcript(&mut self) {
        let Some(path) = self.transcript.clone() else { return };
        match self.tail.read(&path) {
            Ok(batch) => {
                if batch.reset {
                    self.turn_open = false;
                }
                for line in batch.bytes.split(|b| *b == b'\n') {
                    let Ok(entry) = serde_json::from_slice::<Value>(line) else { continue };
                    if entry["type"] == "turn_ended" {
                        self.turn_open = false;
                    } else if entry["role"] == "user" {
                        self.turn_open = true;
                    }
                }
            }
            Err(_) => self.turn_open = false,
        }
    }
}

pub(in crate::agents) fn last_message(store: &Path) -> error::Result<Option<Said>> {
    let failed = |e: rusqlite::Error| Error::Record { path: store.to_path_buf(), cause: e.to_string() };
    let connection = open(store).map_err(failed)?;
    let Some(root) = meta(&connection).and_then(|meta| meta.root).and_then(|id| blob(&connection, &id)) else {
        return Ok(None);
    };
    let Some(turns) = proto::fields(&root).and_then(|fields| blobs_of(&fields, 8)) else { return Ok(None) };
    for turn in turns.iter().rev() {
        let Some(steps) = agent_turn(&connection, turn).and_then(|inner| blobs_of(&proto::fields(&inner)?, 2)) else {
            continue;
        };
        for step in steps.iter().rev() {
            let Some(said) = blob(&connection, step).and_then(|data| assistant_text(&data)) else { continue };
            return Ok(Some(said));
        }
    }
    Ok(None)
}

fn assistant_text(data: &[u8]) -> Option<Said> {
    let step = proto::fields(data)?;
    let reply = proto::fields(proto::bytes(&step, 1)?)?;
    let text = message::joined(proto::text(&reply, 1))?;
    let at = proto::number(&reply, 3).or_else(|| proto::number(&reply, 2)).and_then(moment);
    Some(Said { text, written: at.map(|at| Stamp(at).to_string()) })
}

#[cfg(test)]
mod tests;
