use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, params};
use serde::Deserialize;
use serde_json::Value;

use super::counting::{Record, Usage};
use super::discovery::{folder, named};
use super::message::{self, Said};
use super::{Context, percent};
use crate::error::{self, Error};
use crate::process;
use crate::time::Stamp;

const NAME: &str = "opencode";
const BUSY_FOR: Duration = Duration::from_millis(100);
const MESSAGES: i64 = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Session {
    pub turn: bool,
    pub context: Option<Context>,
    pub prompted: Option<SystemTime>,
    pub place: Option<Place>,
    pub counted: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Place {
    pub database: PathBuf,
    pub id: String,
}

#[derive(Debug, Default)]
pub(super) struct Models {
    last: Option<(Key, Option<Model>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Key {
    file: Option<(PathBuf, SystemTime)>,
    provider: String,
    model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    name: String,
    window: Option<u64>,
}

impl Models {
    fn find(&mut self, file: Option<&Path>, provider: &str, model: &str) -> Option<Model> {
        let file = file.and_then(|path| Some((path.to_path_buf(), std::fs::metadata(path).ok()?.modified().ok()?)));
        let key = Key { file, provider: provider.to_string(), model: model.to_string() };
        if self.last.as_ref().is_none_or(|(last, _)| *last != key) {
            let found = key.file.as_ref().and_then(|(path, _)| read_model(path, provider, model));
            self.last = Some((key, found));
        }
        self.last.as_ref().and_then(|(_, found)| found.clone())
    }
}

#[derive(Deserialize)]
struct Provider {
    #[serde(default)]
    models: HashMap<String, Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: Option<String>,
    limit: Option<Limit>,
}

#[derive(Deserialize)]
struct Limit {
    context: Option<u64>,
}

fn read_model(path: &Path, provider: &str, model: &str) -> Option<Model> {
    let mut providers: HashMap<String, Provider> =
        serde_json::from_reader(BufReader::new(File::open(path).ok()?)).ok()?;
    let entry = providers.remove(provider)?.models.remove(model)?;
    Some(Model {
        name: entry.name.filter(|name| !name.trim().is_empty()).unwrap_or_else(|| model.to_string()),
        window: entry.limit.and_then(|limit| limit.context).filter(|window| *window > 0),
    })
}

pub(super) fn look(pid: i32, since: SystemTime, models: &mut Models) -> Option<Session> {
    let (holder, database) = database(pid)?;
    let cwd = folder(holder)?;
    if shared(holder, &database, &cwd) {
        return None;
    }
    let connection = open(&database).ok()?;
    let since = i64::try_from(since.duration_since(UNIX_EPOCH).ok()?.as_millis()).ok()?;
    let Some(id) = session(&connection, &cwd, since).ok()? else { return Some(Session::default()) };
    let messages = messages(&connection, &id)?;
    let session = read(&messages, models, models_file(holder).as_deref());
    Some(Session { place: Some(Place { database, id }), ..session })
}

fn open(database: &Path) -> rusqlite::Result<Connection> {
    let connection =
        Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    connection.busy_timeout(BUSY_FOR)?;
    Ok(connection)
}

fn database(pid: i32) -> Option<(i32, PathBuf)> {
    std::iter::once(pid).chain(process::descendants(pid).into_iter().filter(|&child| is_opencode(child))).find_map(
        |candidate| {
            let named = configured(candidate);
            let found = process::open_files(candidate).into_iter().find(|path| is_database(path, named.as_deref()))?;
            Some((candidate, found))
        },
    )
}

fn configured(pid: i32) -> Option<String> {
    let env = process::env(pid);
    let value = env.into_iter().find(|(key, value)| key == "OPENCODE_DB" && !value.is_empty())?.1;
    Path::new(&value).file_name()?.to_str().map(str::to_string)
}

fn is_database(path: &Path, named: Option<&str>) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    named.map_or_else(
        || name.starts_with(NAME) && Path::new(name).extension().is_some_and(|e| e == "db"),
        |named| name == named,
    )
}

fn is_opencode(pid: i32) -> bool {
    named(&process::args(pid), NAME)
}

fn shared(holder: i32, database: &Path, cwd: &Path) -> bool {
    let others = process::all();
    let family = process::descendants(holder);
    others.into_iter().any(|other| {
        other != holder
            && !family.contains(&other)
            && is_opencode(other)
            && folder(other).as_deref() == Some(cwd)
            && process::open_files(other).iter().any(|path| path == database)
    })
}

fn models_file(pid: i32) -> Option<PathBuf> {
    let env = process::env(pid);
    let var = |name| env.iter().find(|(key, value)| key == name && !value.is_empty()).map(|(_, v)| PathBuf::from(v));
    let path = var("OPENCODE_MODELS_PATH").or_else(|| {
        let cache = var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))?;
        Some(cache.join(NAME).join("models.json"))
    })?;
    if path.is_absolute() { Some(path) } else { Some(process::cwd(pid)?.join(path)) }
}

fn session(connection: &Connection, cwd: &Path, since: i64) -> rusqlite::Result<Option<String>> {
    let mut statement = connection.prepare(
        "SELECT id, directory FROM session WHERE parent_id IS NULL AND time_archived IS NULL \
         AND time_updated >= ?1 ORDER BY time_updated DESC",
    )?;
    let rows: Vec<(String, String)> =
        statement.query_map(params![since], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<Result<_, _>>()?;
    Ok(rows
        .into_iter()
        .find_map(|(id, directory)| (Path::new(&directory).canonicalize().ok().as_deref() == Some(cwd)).then_some(id)))
}

fn messages(connection: &Connection, id: &str) -> Option<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT data FROM message WHERE session_id = ?1 ORDER BY time_created DESC, id DESC LIMIT ?2")
        .ok()?;
    let rows = statement.query_map(params![id, MESSAGES], |row| row.get::<_, String>(0)).ok()?;
    Some(rows.flatten().collect())
}

#[derive(Debug, Default, Deserialize)]
struct Message {
    role: Option<String>,
    #[serde(rename = "providerID")]
    provider: Option<String>,
    #[serde(rename = "modelID")]
    model: Option<String>,
    #[serde(rename = "model")]
    picked: Option<Picked>,
    time: Option<Times>,
    tokens: Option<Tokens>,
}

#[derive(Debug, Default, Deserialize)]
struct Picked {
    #[serde(rename = "providerID")]
    provider: Option<String>,
    #[serde(rename = "modelID")]
    model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Times {
    created: Option<u64>,
    completed: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tokens {
    input: u64,
    output: u64,
    reasoning: u64,
    cache: Cache,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Cache {
    read: u64,
    write: u64,
}

impl Message {
    fn assistant(&self) -> bool {
        self.role.as_deref() == Some("assistant")
    }

    fn key(&self) -> Option<(&str, &str)> {
        let (provider, model) = if self.assistant() {
            (self.provider.as_deref(), self.model.as_deref())
        } else {
            let picked = self.picked.as_ref()?;
            (picked.provider.as_deref(), picked.model.as_deref())
        };
        Some((provider?, model.filter(|m| !m.trim().is_empty())?))
    }

    fn used(&self) -> Option<u64> {
        let tokens = self.tokens.as_ref().filter(|t| t.output > 0)?;
        let used = [tokens.input, tokens.output, tokens.reasoning, tokens.cache.read, tokens.cache.write]
            .into_iter()
            .fold(0_u64, u64::saturating_add);
        (used > 0).then_some(used)
    }
}

#[derive(Deserialize)]
struct Part {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
    synthetic: Option<bool>,
}

pub(super) fn last_message(database: &Path, session: &str) -> error::Result<Option<Said>> {
    let failed = |e: rusqlite::Error| Error::Record { path: database.to_path_buf(), cause: e.to_string() };
    let connection = open(database).map_err(failed)?;
    let mut messages = connection
        .prepare("SELECT id, data FROM message WHERE session_id = ?1 ORDER BY time_created DESC, id DESC")
        .map_err(failed)?;
    let mut parts = connection.prepare("SELECT data FROM part WHERE message_id = ?1 ORDER BY id").map_err(failed)?;
    let mut rows = messages.query(params![session]).map_err(failed)?;
    while let Some(row) = rows.next().map_err(failed)? {
        let (id, data): (String, String) = (row.get(0).map_err(failed)?, row.get(1).map_err(failed)?);
        let said: Value = serde_json::from_str(&data).unwrap_or_default();
        if said["role"] != "assistant" || said["summary"] == true {
            continue;
        }
        let texts: Vec<String> = parts
            .query_map(params![id], |row| row.get::<_, String>(0))
            .map_err(failed)?
            .flatten()
            .filter_map(|data| serde_json::from_str::<Part>(&data).ok())
            .filter(|part| part.kind.as_deref() == Some("text") && part.synthetic != Some(true))
            .filter_map(|part| part.text)
            .collect();
        let Some(text) = message::joined(texts) else { continue };
        let time = said["time"]["completed"].as_u64().or_else(|| said["time"]["created"].as_u64());
        let written = time.map(|ms| Stamp(UNIX_EPOCH + Duration::from_millis(ms)).to_string());
        return Ok(Some(Said { text, written }));
    }
    Ok(None)
}

fn read(newest_first: &[String], models: &mut Models, file: Option<&Path>) -> Session {
    let messages: Vec<Message> = newest_first.iter().filter_map(|data| serde_json::from_str(data).ok()).collect();
    let turn = messages
        .iter()
        .find(|m| matches!(m.role.as_deref(), Some("user" | "assistant")))
        .is_some_and(|m| !m.assistant() || m.time.as_ref().is_none_or(|t| t.completed.is_none()));
    let context = messages.iter().find_map(Message::key).map(|(provider, model)| {
        let found = models.find(file, provider, model);
        let reply = messages.iter().find(|m| m.assistant() && m.used().is_some());
        let used = reply.filter(|reply| reply.key() == Some((provider, model))).and_then(Message::used);
        Context {
            model: found.as_ref().map_or_else(|| model.to_string(), |found| found.name.clone()),
            percent: used.zip(found.and_then(|found| found.window)).map(|(used, window)| percent(used, window)),
        }
    });
    let prompted = messages
        .iter()
        .filter(|m| m.role.as_deref() == Some("user"))
        .filter_map(|m| m.time.as_ref()?.created)
        .max()
        .and_then(|created| UNIX_EPOCH.checked_add(Duration::from_millis(created)));
    let counted = newest_first.iter().rev().filter_map(|data| counted(data)).collect();
    Session { turn, context, prompted, place: None, counted }
}

fn counted(data: &str) -> Option<Record> {
    let entry: Value = serde_json::from_str(data).ok()?;
    let id = entry["id"].as_str()?.into();
    let (ms, usage) = match entry["role"].as_str()? {
        "user" => (entry["time"]["created"].as_u64()?, Usage::Prompt),
        "assistant" => {
            let v = entry.get("tokens")?;
            let tokens = crate::stats::Tokens {
                input: v["input"].as_u64().unwrap_or(0),
                out: v["output"].as_u64().unwrap_or(0).saturating_add(v["reasoning"].as_u64().unwrap_or(0)),
                cache_read: v["cache"]["read"].as_u64().unwrap_or(0),
                cache_write: v["cache"]["write"].as_u64().unwrap_or(0),
            };
            (entry["time"]["completed"].as_u64()?, Usage::Completed(tokens))
        }
        _ => return None,
    };
    Some(Record { at: UNIX_EPOCH.checked_add(Duration::from_millis(ms))?, id, usage })
}

#[cfg(test)]
mod tests;
