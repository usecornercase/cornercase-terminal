use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Deserialize;
use serde_json::Value;

use crate::agents;
use crate::agents::activity::Claude;
use crate::process;

mod codex;
mod counting;
mod discovery;
mod gemini;
mod jsonl;
mod message;
mod opencode;

pub(crate) use message::{Record, Said};

use counting::{Ledger, Record as CountRecord, Usage as CountUsage};
use discovery::Job;
use jsonl::Tail;

const TAIL: u64 = 1024 * 1024;
const DIR_NAME_MAX: usize = 200;
const STANDARD_WINDOW: u64 = 200_000;
const LONG_WINDOW: u64 = 1_000_000;
const LONG_SUFFIX: &str = "[1m]";
const SYNTHETIC: &str = "<synthetic>";
const ASSISTANT: &str = r#""type":"assistant""#;
const COMPACT_BOUNDARY: &str = r#""compact_boundary""#;
const COMMAND: &str = "<command-name>";
const SHELL_INPUT: &str = "<bash-input>";
const NOT_TYPED: [&str; 2] = ["<task-notification>", "<agent-message"];
const MODEL_FLAG: &str = "--model";
const MODEL_ENV: &str = "ANTHROPIC_MODEL";
pub(crate) const NO_LONG_ENV: &str = "CLAUDE_CODE_DISABLE_1M_CONTEXT";
const MAX_TOKENS_ENV: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";
pub(crate) const NO_COMPACT_ENV: &str = "DISABLE_COMPACT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) model: String,
    pub(crate) percent: Option<u16>,
}

#[derive(Debug, Default)]
pub(crate) struct Pane {
    transcript: Option<Transcript>,
    shown: Option<Context>,
    codex: Option<codex::Rollout>,
    agent: Option<(i32, SystemTime)>,
    kind: Option<&'static str>,
    finding: Job<Option<PathBuf>>,
    opencode: Option<opencode::Session>,
    looking: Job<(Option<opencode::Session>, opencode::Models)>,
    models: opencode::Models,
    gemini: Option<gemini::Session>,
    reading: Job<Option<gemini::Session>>,
    gemini_record: Option<(PathBuf, Option<String>)>,
    prompted: Option<SystemTime>,
    ledger: Ledger,
    subagents: Subagents,
    following: Job<Subagents>,
}

impl Pane {
    fn follow(&mut self, pid: i32, kind: &'static str) {
        if self.agent.is_none_or(|(seen, _)| seen != pid) || self.kind != Some(kind) {
            *self = Self { agent: Some((pid, SystemTime::now())), kind: Some(kind), ..Self::default() };
        }
    }

    pub(crate) fn kind(&self) -> Option<&'static str> {
        self.kind
    }

    pub(crate) fn agent(&self) -> Option<(i32, SystemTime)> {
        self.agent
    }

    pub(crate) fn take_counted(&mut self) -> agents::stats::Counted {
        self.ledger.take()
    }

    pub(crate) fn update_codex(&mut self, pid: i32) {
        self.follow(pid, agents::CODEX);
        if let Some(found) = self.finding.take()
            && found.as_ref() != self.codex.as_ref().map(|r| &r.path)
        {
            self.codex = found.map(codex::Rollout::new);
            self.shown = None;
        }
        if let Some((pid, since)) = self.agent {
            self.finding.start(move || codex::rollout_path(pid, since));
        }
        if let Some(rollout) = &mut self.codex {
            rollout.update();
            self.shown = rollout.context();
            self.prompted = self.prompted.max(rollout.prompted());
            if let Some((_, since)) = self.agent {
                self.ledger.apply(&rollout.path.to_string_lossy(), since, std::mem::take(&mut rollout.counted));
            }
        }
    }

    pub(crate) fn update_opencode(&mut self, pid: i32) {
        self.follow(pid, agents::OPENCODE);
        if let Some((mut found, models)) = self.looking.take() {
            if let Some(session) = &mut found
                && let Some((_, since)) = self.agent
                && let Some(place) = &session.place
            {
                self.ledger.apply(&place.id, since, std::mem::take(&mut session.counted));
            }
            self.models = models;
            self.shown = found.as_ref().and_then(|session| session.context.clone());
            self.prompted = self.prompted.max(found.as_ref().and_then(|session| session.prompted));
            self.opencode = found;
        }
        if !self.looking.running()
            && let Some((pid, since)) = self.agent
        {
            let mut models = std::mem::take(&mut self.models);
            self.looking.start(move || (opencode::look(pid, since, &mut models), models));
        }
    }

    pub(crate) fn update_gemini(&mut self, pid: i32, screen: &str) {
        self.follow(pid, agents::GEMINI);
        if let Some(mut found) = self.reading.take() {
            if let Some(session) = &mut found
                && let Some((_, since)) = self.agent
            {
                self.ledger.apply(&session.path.to_string_lossy(), since, std::mem::take(&mut session.counted));
            }
            self.shown = found.as_ref().and_then(|session| session.context.clone());
            self.prompted = self.prompted.max(found.as_ref().and_then(|session| session.prompted));
            self.gemini_record = found.as_ref().map(|s| (s.path.clone(), s.id().map(str::to_string)));
            self.gemini = found;
        }
        if !self.reading.running()
            && let Some((pid, since)) = self.agent
        {
            let previous = self.gemini.take();
            let footer = gemini::footer_model(screen);
            self.reading.start(move || gemini::look(pid, since, previous, footer));
        }
    }

    pub(crate) fn update(&mut self, dir: Option<&Path>, claude: Option<&Claude>) {
        if let Some(claude) = claude {
            self.follow(claude.pid, agents::CLAUDE);
        }
        let found = dir.zip(claude).and_then(|(dir, claude)| {
            let session = claude.session.as_ref()?;
            let cwd = session.cwd.as_deref()?;
            Some((dir, claude, cwd, transcript_path(dir, cwd, session.id.as_deref()?)?))
        });
        let Some((dir, claude, cwd, path)) = found else {
            if claude.is_none() {
                *self = Self::default();
            } else {
                self.transcript = None;
                self.shown = None;
            }
            return;
        };
        if self.transcript.as_ref().is_none_or(|t| t.path != path) {
            self.transcript = None;
            self.shown = None;
        }
        let transcript = self.transcript.get_or_insert_with(|| Transcript::new(path));
        let before = transcript.reply.clone();
        transcript.update();
        self.prompted = self.prompted.max(transcript.prompted);
        if let Some((_, since)) = self.agent {
            self.ledger.apply(&transcript.path.to_string_lossy(), since, std::mem::take(&mut transcript.counted));
        }
        if transcript.reply != before {
            self.shown = transcript.reply.as_ref().map(|reply| reply.context(&Limits::read(dir, cwd, claude)));
        }
        let parent = transcript.path.with_extension("").join("subagents");
        if let Some(mut found) = self.following.take() {
            if let Some((_, since)) = self.agent {
                for (path, records) in std::mem::take(&mut found.records) {
                    self.ledger.apply(&path.to_string_lossy(), since, records);
                }
            }
            self.subagents = found;
        }
        if !self.following.running()
            && let Some((_, since)) = self.agent
        {
            let state = std::mem::take(&mut self.subagents);
            self.following.start(move || state.read(&parent, since));
        }
    }

    pub(crate) fn context(&self) -> Option<&Context> {
        self.shown.as_ref()
    }

    pub(crate) fn prompted(&self) -> Option<SystemTime> {
        self.prompted
    }

    pub(crate) fn codex_turn(&self) -> bool {
        self.codex.as_ref().is_some_and(codex::Rollout::turn)
    }

    pub(crate) fn opencode_turn(&self) -> Option<bool> {
        self.opencode.as_ref().map(|session| session.turn)
    }

    pub(crate) fn record_for(&self, agent: &str) -> Option<Record> {
        Some(match agent {
            agents::CLAUDE => Record::Claude(self.transcript.as_ref()?.path.clone()),
            agents::CODEX => Record::Codex(self.codex.as_ref()?.path.clone()),
            agents::GEMINI => Record::Gemini(self.gemini_record.as_ref()?.0.clone()),
            agents::OPENCODE => {
                let place = self.opencode.as_ref()?.place.clone()?;
                Record::Opencode { database: place.database, session: place.id }
            }
            _ => return None,
        })
    }

    pub(crate) fn conversation(&self) -> Option<&str> {
        let claude = self.transcript.as_ref().filter(|t| t.len > 0).and_then(|t| t.path.file_stem()?.to_str());
        claude.or_else(|| self.codex.as_ref()?.id()).or_else(|| self.gemini_record.as_ref()?.1.as_deref())
    }
}

#[derive(Debug, Default)]
struct Subagents {
    directory: Option<(PathBuf, SystemTime)>,
    tails: HashMap<PathBuf, Tail>,
    records: Vec<(PathBuf, Vec<CountRecord>)>,
}

impl Subagents {
    fn read(mut self, parent: &Path, since: SystemTime) -> Self {
        let directory = std::fs::metadata(parent).and_then(|m| m.modified()).ok().map(|at| (parent.to_path_buf(), at));
        if directory != self.directory {
            let mut paths = Vec::new();
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.file_name().and_then(|s| s.to_str()).is_some_and(|s| s.starts_with("agent-"))
                        && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
                    {
                        paths.push(path);
                    }
                }
            }
            self.tails.retain(|path, _| paths.contains(path));
            for path in paths {
                self.tails.entry(path).or_default();
            }
            self.directory = directory;
        }
        for (path, tail) in &mut self.tails {
            if let Ok(batch) = tail.read_since(path, Some(since)) {
                let records = batch
                    .bytes
                    .split(|b| *b == b'\n')
                    .filter_map(claude_entry)
                    .filter_map(|entry| counted_claude(&entry, false))
                    .collect();
                self.records.push((path.clone(), records));
            }
        }
        self
    }
}

fn transcript_path(dir: &Path, cwd: &Path, id: &str) -> Option<PathBuf> {
    let safe = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    safe.then(|| dir.join("projects").join(project_dir(cwd)).join(format!("{id}.jsonl")))
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
struct Transcript {
    path: PathBuf,
    len: u64,
    tail: Tail,
    counted: Vec<CountRecord>,
    reply: Option<Reply>,
    prompted: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Reply {
    model: String,
    tokens: u64,
}

impl Reply {
    fn context(&self, limits: &Limits) -> Context {
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
    fn new(path: PathBuf) -> Self {
        Self { path, len: 0, tail: Tail::default(), counted: Vec::new(), reply: None, prompted: None }
    }

    fn update(&mut self) {
        let Ok(batch) = self.tail.read(&self.path) else {
            self.reply = None;
            self.len = 0;
            self.tail = Tail::default();
            return;
        };
        if batch.reset {
            self.reply = None;
            self.prompted = None;
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
            self.prompted = self.prompted.max(prompt(&entry, read_at));
            if let Some(record) = counted_claude(&entry, true) {
                self.counted.push(record);
            }
        }
    }
}

fn relevant_claude(line: &str) -> bool {
    [r#""usage""#, r#""origin""#, "queued_command", COMMAND, SHELL_INPUT, r#""operation""#, COMPACT_BOUNDARY]
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
struct Limits {
    model: Option<String>,
    no_long: bool,
    max_tokens: Option<u64>,
    no_compact: bool,
}

impl Limits {
    fn read(dir: &Path, cwd: &Path, claude: &Claude) -> Self {
        let project = cwd.join(".claude");
        let files: Vec<Value> =
            [dir.join("settings.json"), project.join("settings.json"), project.join("settings.local.json")]
                .iter()
                .filter_map(|path| serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok())
                .collect();
        let mut env: HashMap<String, String> = process::env(claude.pid).into_iter().collect();
        for vars in files.iter().filter_map(|file| file.get("env")?.as_object()) {
            env.extend(vars.iter().filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_string()))));
        }
        let model = files.iter().rev().find_map(|file| file.get("model")?.as_str().map(str::to_string));
        Self::new(&claude.args, &env, model)
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

fn percent(tokens: u64, window: u64) -> u16 {
    let window = window.max(1);
    let rounded = tokens.saturating_mul(100).saturating_add(window / 2) / window;
    u16::try_from(rounded.min(100)).unwrap_or(100)
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
