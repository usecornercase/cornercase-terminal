use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;
use serde_json::Value;

use super::MODEL_FLAG;
use crate::agents::context::counting::{self, Record as CountRecord, Usage as CountUsage};
use crate::agents::context::jsonl::{Batch, Tail};
use crate::agents::context::{Context, Look, fresh, percent};
use crate::process;

const DIR_NAME_MAX: usize = 200;
const STANDARD_WINDOW: u64 = 200_000;
const LONG_WINDOW: u64 = 1_000_000;
const LONG_SUFFIX: &str = "[1m]";
pub(in crate::agents) const SYNTHETIC: &str = "<synthetic>";
pub(in crate::agents) const ASSISTANT: &str = r#""type":"assistant""#;
const COMPACT_BOUNDARY: &str = r#""compact_boundary""#;
const COMMAND: &str = "<command-name>";
const SHELL_INPUT: &str = "<bash-input>";
const USER: &str = r#""type":"user""#;
const INTERRUPTED: &str = "[Request interrupted by user";
const NOT_TYPED: [&str; 2] = ["<task-notification>", "<agent-message"];
const MODEL_ENV: &str = "ANTHROPIC_MODEL";
pub(crate) const NO_LONG_ENV: &str = "CLAUDE_CODE_DISABLE_1M_CONTEXT";
const MAX_TOKENS_ENV: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";
pub(crate) const NO_COMPACT_ENV: &str = "DISABLE_COMPACT";

#[derive(Debug, Default)]
struct Child {
    tail: Tail,
    written: Option<SystemTime>,
    replied: bool,
    meta: Option<(Option<SystemTime>, Meta)>,
}

#[derive(Debug, Default, Deserialize)]
struct Meta {
    #[serde(rename = "requestShape")]
    shape: Option<String>,
    finished: Option<bool>,
    #[serde(rename = "stoppedByUser")]
    stopped: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::agents) struct Running {
    written: SystemTime,
    replied: bool,
    ended: bool,
    background: bool,
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Subagents {
    directory: Option<(PathBuf, SystemTime)>,
    children: HashMap<PathBuf, Child>,
    pub(in crate::agents) records: Vec<(PathBuf, Vec<CountRecord>)>,
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn agent_files(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with("agent-"))
                && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        })
        .collect()
}

impl Child {
    fn refresh_meta(&mut self, path: &Path) {
        self.written = modified(path);
        let meta_path = path.with_extension("meta.json");
        let stamp = modified(&meta_path);
        if self.meta.as_ref().is_none_or(|(known, _)| *known != stamp) {
            let meta = std::fs::read_to_string(&meta_path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            self.meta = Some((stamp, meta));
        }
    }

    fn records_of(&mut self, batch: &Batch) -> Vec<CountRecord> {
        if batch.reset {
            self.replied = false;
        }
        let mut records = Vec::new();
        for line in batch.bytes.split(|b| *b == b'\n') {
            if line.windows(USER.len()).any(|w| w == USER.as_bytes()) {
                self.replied = false;
            }
            let Some(entry) = claude_entry(line) else { continue };
            if entry["type"] == "assistant" {
                self.replied = matches!(entry["message"]["stop_reason"].as_str(), Some("end_turn" | "stop_sequence"));
            }
            records.extend(counted_claude(&entry, false));
        }
        records
    }
}

impl Subagents {
    pub(in crate::agents) fn read(mut self, parent: &Path, since: SystemTime) -> Self {
        self.refresh_directory(parent);
        for (path, child) in &mut self.children {
            child.refresh_meta(path);
            if let Ok(batch) = child.tail.read_since(path, Some(since)) {
                self.records.push((path.clone(), child.records_of(&batch)));
            }
        }
        self
    }

    fn refresh_directory(&mut self, parent: &Path) {
        let directory = modified(parent).map(|at| (parent.to_path_buf(), at));
        if directory == self.directory {
            return;
        }
        let paths = agent_files(parent);
        self.children.retain(|path, _| paths.contains(path));
        for path in paths {
            self.children.entry(path).or_default();
        }
        self.directory = directory;
    }

    pub(in crate::agents) fn running(&self) -> Vec<Running> {
        self.children
            .values()
            .filter_map(|child| {
                let meta = child.meta.as_ref().map(|(_, meta)| meta);
                Some(Running {
                    written: child.written?,
                    replied: child.replied,
                    ended: meta.is_some_and(|m| m.finished == Some(true) || m.stopped == Some(true)),
                    background: meta.is_some_and(|m| m.shape.as_deref() == Some("background")),
                })
            })
            .collect()
    }
}

pub(in crate::agents) fn live(
    children: &[Running],
    since: SystemTime,
    busy: bool,
    interrupted: Option<SystemTime>,
    now: SystemTime,
) -> usize {
    children
        .iter()
        .filter(|child| {
            child.written >= since
                && fresh(child.written, now)
                && !child.ended
                && !child.replied
                && interrupted.is_none_or(|at| child.written > at)
                && (child.background || busy)
        })
        .count()
}

pub(in crate::agents) fn transcript_path(dir: &Path, cwd: &Path, id: &str) -> Option<PathBuf> {
    let safe = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    safe.then(|| dir.join("projects").join(project_dir(cwd)).join(format!("{id}.jsonl")))
}

pub(in crate::agents) fn conversation_files(transcript: &Path) -> Option<Vec<(PathBuf, PathBuf)>> {
    let relative = crate::agents::context::under(transcript, "projects")?;
    let mut files = vec![(transcript.to_path_buf(), relative.clone())];
    let folder = transcript.with_extension("");
    if folder.is_dir() {
        files.push((folder, relative.with_extension("")));
    }
    Some(files)
}

fn project_dir(cwd: &Path) -> String {
    let cwd = cwd.to_string_lossy();
    let name: String = cwd
        .encode_utf16()
        .map(|unit| u8::try_from(unit).ok().filter(u8::is_ascii_alphanumeric).map_or('-', char::from))
        .collect();
    if name.len() <= DIR_NAME_MAX {
        return name;
    }
    format!("{}-{}", &name[..DIR_NAME_MAX], base36(string_hash(&cwd)))
}

fn string_hash(text: &str) -> u32 {
    text.encode_utf16()
        .fold(0_i32, |hash, unit| hash.wrapping_shl(5).wrapping_sub(hash).wrapping_add(i32::from(unit)))
        .unsigned_abs()
}

fn base36(mut n: u32) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(char::from_digit(n % 36, 36).unwrap_or('0'));
        n /= 36;
        if n == 0 {
            return digits.iter().rev().collect();
        }
    }
}

#[derive(Debug)]
pub(in crate::agents) struct Transcript {
    pub(in crate::agents) path: PathBuf,
    pub(in crate::agents) len: u64,
    tail: Tail,
    pub(in crate::agents) counted: Vec<CountRecord>,
    pub(in crate::agents) reply: Option<Reply>,
    pub(in crate::agents) prompted: Option<SystemTime>,
    pub(in crate::agents) interrupted: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::agents) struct Reply {
    model: String,
    tokens: u64,
}

impl Reply {
    pub(in crate::agents) fn context(&self, limits: &Limits) -> Context {
        let window = limits.window(&self.model, self.tokens);
        Context { model: model_name(&self.model), percent: Some(percent(self.tokens, window)) }
    }
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    subtype: Option<String>,
    #[serde(rename = "isSidechain")]
    sidechain: Option<bool>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(rename = "input_tokens")]
    input: Option<u64>,
    #[serde(rename = "cache_creation_input_tokens")]
    cache_creation: Option<u64>,
    #[serde(rename = "cache_read_input_tokens")]
    cache_read: Option<u64>,
}

impl Message {
    fn reply(self) -> Option<Reply> {
        let model = self.model.filter(|m| !m.is_empty() && m != SYNTHETIC)?;
        let usage = self.usage?;
        let tokens = [usage.input, usage.cache_creation, usage.cache_read].into_iter().flatten().sum();
        (tokens > 0).then_some(Reply { model, tokens })
    }
}

impl Transcript {
    pub(in crate::agents) fn new(path: PathBuf) -> Self {
        Self {
            path,
            len: 0,
            tail: Tail::default(),
            counted: Vec::new(),
            reply: None,
            prompted: None,
            interrupted: None,
        }
    }

    pub(in crate::agents) fn update(&mut self) {
        let Ok(batch) = self.tail.read(&self.path) else {
            self.reply = None;
            self.interrupted = None;
            self.len = 0;
            self.tail = Tail::default();
            return;
        };
        if batch.reset {
            self.reply = None;
            self.prompted = None;
            self.interrupted = None;
        }
        self.len = std::fs::metadata(&self.path).map_or(0, |m| m.len());
        if let Some(mark) = batch.bytes.rsplit(|b| *b == b'\n').find_map(mark) {
            self.reply = match mark {
                Mark::Reply(reply) => Some(reply),
                Mark::Compacted => None,
            };
        }
        let read_at = SystemTime::now();
        for line in batch.bytes.split(|b| *b == b'\n') {
            let Some(entry) = claude_entry(line) else { continue };
            if entry["type"] == "user"
                && entry["isSidechain"] != true
                && std::str::from_utf8(line).is_ok_and(|line| line.contains(INTERRUPTED))
            {
                self.interrupted = self.interrupted.max(counting::timestamp(&entry).or(Some(read_at)));
            }
            self.prompted = self.prompted.max(prompt(&entry, read_at));
            if let Some(record) = counted_claude(&entry, true) {
                self.counted.push(record);
            }
        }
    }
}

fn relevant_claude(line: &str) -> bool {
    [
        r#""usage""#,
        r#""origin""#,
        "queued_command",
        COMMAND,
        SHELL_INPUT,
        r#""operation""#,
        COMPACT_BOUNDARY,
        INTERRUPTED,
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

fn claude_entry(line: &[u8]) -> Option<Value> {
    let line = std::str::from_utf8(line).ok()?;
    relevant_claude(line).then(|| serde_json::from_str(line).ok()).flatten()
}

fn text(entry: &Value) -> &str {
    entry["message"]["content"].as_str().or_else(|| entry["content"].as_str()).unwrap_or_default()
}

fn prompt(entry: &Value, read_at: SystemTime) -> Option<SystemTime> {
    if entry["isSidechain"] == true || entry["isMeta"] == true {
        return None;
    }
    let text = text(entry);
    let typed = match entry["type"].as_str()? {
        "user" if entry["origin"]["kind"] == "human" => true,
        "user" | "system" => text.starts_with(COMMAND) || text.starts_with(SHELL_INPUT),
        "queue-operation" => entry["operation"] == "enqueue" && !NOT_TYPED.iter().any(|tag| text.starts_with(tag)),
        _ => false,
    };
    typed.then(|| counting::timestamp(entry).unwrap_or(read_at))
}

fn counted_claude(entry: &Value, prompts: bool) -> Option<CountRecord> {
    let at = counting::timestamp(entry)?;
    let usage = &entry["message"]["usage"];
    if entry["type"] == "assistant" && usage.is_object() && entry["message"]["model"] != SYNTHETIC {
        return Some(CountRecord {
            at,
            id: entry["message"]["id"].as_str()?.into(),
            usage: CountUsage::Tokens(counting::tokens(
                usage,
                "input_tokens",
                "output_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            )),
        });
    }
    let queued = entry["type"] == "attachment"
        && entry["attachment"]["type"] == "queued_command"
        && entry["attachment"]["commandMode"] == "prompt"
        && entry["attachment"]["origin"]["kind"] == "human";
    let typed = entry["origin"]["kind"] != "peer"
        && matches!(entry["type"].as_str(), Some("user" | "system"))
        && !text(entry).starts_with(SHELL_INPUT)
        && prompt(entry, at).is_some();
    if prompts && entry["isSidechain"] != true && entry["isMeta"] != true && (queued || typed) {
        let id = entry["uuid"]
            .as_str()
            .or_else(|| entry["id"].as_str())
            .map_or_else(|| format!("{:?}:{}", at, entry["type"]), str::to_string);
        return Some(CountRecord { at, id, usage: CountUsage::Prompt });
    }
    None
}

enum Mark {
    Reply(Reply),
    Compacted,
}

fn mark(line: &[u8]) -> Option<Mark> {
    let line = std::str::from_utf8(line).ok()?;
    if !(line.contains(COMPACT_BOUNDARY) || line.contains(ASSISTANT) && line.contains(r#""usage""#)) {
        return None;
    }
    let entry: Entry = serde_json::from_str(line).ok()?;
    match entry.kind.as_deref()? {
        "system" if entry.subtype.as_deref() == Some("compact_boundary") => Some(Mark::Compacted),
        "assistant" if entry.sidechain != Some(true) => entry.message?.reply().map(Mark::Reply),
        _ => None,
    }
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Limits {
    model: Option<String>,
    no_long: bool,
    max_tokens: Option<u64>,
    no_compact: bool,
}

impl Limits {
    pub(in crate::agents) fn read(dir: &Path, cwd: &Path, look: &Look<'_>) -> Self {
        let project = cwd.join(".claude");
        let files: Vec<Value> =
            [dir.join("settings.json"), project.join("settings.json"), project.join("settings.local.json")]
                .iter()
                .filter_map(|path| serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok())
                .collect();
        let mut env: HashMap<String, String> = process::env(look.pid).into_iter().collect();
        for vars in files.iter().filter_map(|file| file.get("env")?.as_object()) {
            env.extend(vars.iter().filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string()))));
        }
        let model = files.iter().rev().find_map(|file| file.get("model")?.as_str().map(str::to_string));
        Self::new(&look.args, &env, model)
    }

    fn new(args: &[String], env: &HashMap<String, String>, settings_model: Option<String>) -> Self {
        let var = |name: &str| env.get(name).map(|value| value.trim()).filter(|value| !value.is_empty());
        Self {
            model: model_flag(args).or_else(|| var(MODEL_ENV).map(str::to_string)).or(settings_model),
            no_long: var(NO_LONG_ENV).is_some_and(truthy),
            max_tokens: var(MAX_TOKENS_ENV).and_then(|value| value.parse().ok()).filter(|n| *n > 0),
            no_compact: var(NO_COMPACT_ENV).is_some_and(truthy),
        }
    }

    fn window(&self, model: &str, tokens: u64) -> u64 {
        if self.no_compact
            && let Some(max) = self.max_tokens
        {
            return max;
        }
        let known = Model::parse(model);
        let long = known.as_ref().is_some_and(Model::native_long)
            || self.model.as_deref().is_some_and(|setting| asks_long(setting, model));
        if long && !self.no_long {
            return LONG_WINDOW;
        }
        if known.is_none()
            && let Some(max) = self.max_tokens
        {
            return max;
        }
        if tokens > STANDARD_WINDOW { LONG_WINDOW } else { STANDARD_WINDOW }
    }
}

fn model_flag(args: &[String]) -> Option<String> {
    let mut found = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == MODEL_FLAG {
            found = args.next().cloned();
        } else if let Some(model) = arg.strip_prefix(MODEL_FLAG).and_then(|rest| rest.strip_prefix('=')) {
            found = Some(model.to_string());
        }
    }
    found
}

fn asks_long(setting: &str, model: &str) -> bool {
    let setting = setting.trim().to_ascii_lowercase();
    setting.strip_suffix(LONG_SUFFIX).is_some_and(|base| !base.is_empty() && model.to_ascii_lowercase().contains(base))
}

fn truthy(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

#[derive(Debug)]
struct Model {
    family: String,
    version: Vec<u32>,
}

impl Model {
    fn parse(id: &str) -> Option<Self> {
        let lower = id.trim().to_ascii_lowercase();
        let id = lower.rsplit('/').next().unwrap_or_default();
        let id = id.split_once("anthropic.").map_or(id, |(_, rest)| rest);
        let id = id.split(['@', '[']).next().unwrap_or_default();
        let tokens: Vec<&str> = id.strip_prefix("claude-")?.split('-').collect();
        let family = tokens.iter().find(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_alphabetic()))?;
        let version = tokens
            .iter()
            .filter(|t| (1..=2).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit()))
            .filter_map(|t| t.parse().ok())
            .collect();
        Some(Self { family: (*family).to_string(), version })
    }

    fn native_long(&self) -> bool {
        let major = self.version.first().copied().unwrap_or(0);
        let minor = self.version.get(1).copied().unwrap_or(0);
        major >= 5 || (self.family == "opus" && (major, minor) >= (4, 7))
    }

    fn name(&self) -> String {
        let mut family = self.family.clone();
        if let Some(first) = family.get_mut(..1) {
            first.make_ascii_uppercase();
        }
        let version = match self.version.as_slice() {
            [major, 0] => major.to_string(),
            parts => parts.iter().map(u32::to_string).collect::<Vec<_>>().join("."),
        };
        if version.is_empty() { family } else { format!("{family} {version}") }
    }
}

fn model_name(id: &str) -> String {
    Model::parse(id).map_or_else(|| id.to_string(), |model| model.name())
}

#[cfg(test)]
mod tests;
