use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::SystemTime;

use serde::Deserialize;
use serde_json::Value;

use crate::activity::Claude;
use crate::process;

mod codex;
mod opencode;

const TAIL: u64 = 1024 * 1024;
const DIR_NAME_MAX: usize = 200;
const STANDARD_WINDOW: u64 = 200_000;
const LONG_WINDOW: u64 = 1_000_000;
const LONG_SUFFIX: &str = "[1m]";
const SYNTHETIC: &str = "<synthetic>";
const ASSISTANT: &str = r#""type":"assistant""#;
const COMPACT_BOUNDARY: &str = r#""compact_boundary""#;
const MODEL_FLAG: &str = "--model";
const MODEL_ENV: &str = "ANTHROPIC_MODEL";
pub const NO_LONG_ENV: &str = "CLAUDE_CODE_DISABLE_1M_CONTEXT";
const MAX_TOKENS_ENV: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";
pub const NO_COMPACT_ENV: &str = "DISABLE_COMPACT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub model: String,
    pub percent: Option<u16>,
}

#[derive(Debug, Default)]
pub struct Pane {
    transcript: Option<Transcript>,
    shown: Option<Context>,
    codex: Option<codex::Rollout>,
    agent: Option<(i32, SystemTime)>,
    finding: Option<Receiver<Option<PathBuf>>>,
    opencode: Option<opencode::Session>,
    looking: Option<Receiver<(Option<opencode::Session>, opencode::Models)>>,
    models: opencode::Models,
}

impl Pane {
    pub fn update_codex(&mut self, pid: i32) {
        if self.agent.is_none_or(|(seen, _)| seen != pid) {
            *self = Self { agent: Some((pid, SystemTime::now())), ..Self::default() };
        }
        match self.finding.as_ref().map(Receiver::try_recv) {
            Some(Ok(found)) => {
                self.finding = None;
                if found.as_ref() != self.codex.as_ref().map(|r| &r.path) {
                    self.codex = found.map(codex::Rollout::new);
                    self.shown = None;
                }
            }
            Some(Err(TryRecvError::Disconnected)) => self.finding = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        if self.finding.is_none()
            && let Some((pid, since)) = self.agent
        {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || tx.send(codex::rollout_path(pid, since)));
            self.finding = Some(rx);
        }
        if let Some(rollout) = &mut self.codex {
            rollout.update();
            self.shown = rollout.context();
        }
    }

    pub fn update_opencode(&mut self, pid: i32) {
        if self.agent.is_none_or(|(seen, _)| seen != pid) {
            *self = Self { agent: Some((pid, SystemTime::now())), ..Self::default() };
        }
        match self.looking.as_ref().map(Receiver::try_recv) {
            Some(Ok((found, models))) => {
                self.looking = None;
                self.models = models;
                self.shown = found.as_ref().and_then(|session| session.context.clone());
                self.opencode = found;
            }
            Some(Err(TryRecvError::Disconnected)) => self.looking = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        if self.looking.is_none()
            && let Some((pid, since)) = self.agent
        {
            let mut models = std::mem::take(&mut self.models);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let found = opencode::look(pid, since, &mut models);
                tx.send((found, models)).ok();
            });
            self.looking = Some(rx);
        }
    }

    pub fn update(&mut self, dir: Option<&Path>, claude: Option<&Claude>) {
        let found = dir.zip(claude).and_then(|(dir, claude)| {
            let session = claude.session.as_ref()?;
            let cwd = session.cwd.as_deref()?;
            Some((dir, claude, cwd, transcript_path(dir, cwd, session.id.as_deref()?)?))
        });
        let Some((dir, claude, cwd, path)) = found else {
            *self = Self::default();
            return;
        };
        if self.transcript.as_ref().is_none_or(|t| t.path != path) {
            *self = Self::default();
        }
        let transcript = self.transcript.get_or_insert_with(|| Transcript::new(path));
        let before = transcript.reply.clone();
        transcript.update();
        if transcript.reply != before {
            self.shown = transcript.reply.as_ref().map(|reply| reply.context(&Limits::read(dir, cwd, claude)));
        }
    }

    pub fn context(&self) -> Option<&Context> {
        self.shown.as_ref()
    }

    pub fn codex_turn(&self) -> bool {
        self.codex.as_ref().is_some_and(codex::Rollout::turn)
    }

    pub fn opencode_turn(&self) -> Option<bool> {
        self.opencode.as_ref().map(|session| session.turn)
    }

    pub fn conversation(&self) -> Option<&str> {
        let claude = self.transcript.as_ref().filter(|t| t.len > 0).and_then(|t| t.path.file_stem()?.to_str());
        claude.or_else(|| self.codex.as_ref()?.id())
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
    read: u64,
    reply: Option<Reply>,
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
        Self { path, len: 0, read: 0, reply: None }
    }

    fn update(&mut self) {
        let Ok(len) = std::fs::metadata(&self.path).map(|m| m.len()) else {
            *self = Self::new(std::mem::take(&mut self.path));
            return;
        };
        if len == self.len {
            return;
        }
        if len < self.len {
            *self = Self::new(std::mem::take(&mut self.path));
        }
        let Ok(mut file) = File::open(&self.path) else { return };
        self.len = len;
        let jump = self.reply.is_none() && (self.read == 0 || len - self.read > TAIL);
        let start = if jump { len.saturating_sub(TAIL) } else { self.read };
        let mut bytes = Vec::new();
        if file.seek(SeekFrom::Start(start)).is_err() || file.take(len - start).read_to_end(&mut bytes).is_err() {
            return;
        }
        let Some(end) = bytes.iter().rposition(|b| *b == b'\n').map(|i| i + 1) else { return };
        let from = if jump && start > 0 { bytes.iter().position(|b| *b == b'\n').map_or(end, |i| i + 1) } else { 0 };
        if let Some(mark) = bytes[from..end].rsplit(|b| *b == b'\n').find_map(mark) {
            self.reply = match mark {
                Mark::Reply(reply) => Some(reply),
                Mark::Compacted => None,
            };
        }
        self.read = start + end as u64;
    }
}

enum Mark {
    Reply(Reply),
    Compacted,
}

fn mark(line: &[u8]) -> Option<Mark> {
    let line = std::str::from_utf8(line).ok()?;
    if !line.contains(ASSISTANT) && !line.contains(COMPACT_BOUNDARY) {
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
mod tests {
    use std::io::Write;

    use rstest::rstest;

    use super::*;
    use crate::activity::Session;
    use crate::test_util::{Sleeper, TempDir};

    fn assistant(model: &str, input: u64, created: u64, read: u64) -> String {
        format!(
            r#"{{"parentUuid":"p","isSidechain":false,"type":"assistant","message":{{"model":"{model}","role":"assistant","content":[{{"type":"text","text":"ok"}}],"usage":{{"input_tokens":{input},"cache_creation_input_tokens":{created},"cache_read_input_tokens":{read},"output_tokens":9}}}},"uuid":"u"}}"#
        )
    }

    fn user(text: &str) -> String {
        format!(r#"{{"isSidechain":false,"type":"user","message":{{"role":"user","content":"{text}"}},"uuid":"u"}}"#)
    }

    const COMPACTED: &str = r#"{"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"level":"info","compactMetadata":{"trigger":"manual","preTokens":180000}}"#;

    fn reply(model: &str, tokens: u64) -> Reply {
        Reply { model: model.into(), tokens }
    }

    fn joined(lines: &[String]) -> String {
        lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|a| (*a).to_string()).collect()
    }

    fn write_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
        std::fs::write(path, text).expect("write the file");
    }

    struct Written {
        dir: TempDir,
        transcript: Transcript,
    }

    impl Written {
        fn new() -> Self {
            let dir = TempDir::new();
            let transcript = Transcript::new(dir.path().join("session.jsonl"));
            Self { dir, transcript }
        }

        fn append(&mut self, text: &str) -> Option<Reply> {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.path().join("session.jsonl"))
                .expect("open the transcript");
            file.write_all(text.as_bytes()).expect("write the transcript");
            self.transcript.update();
            self.transcript.reply.clone()
        }

        fn lines(&mut self, lines: &[String]) -> Option<Reply> {
            self.append(&joined(lines))
        }
    }

    mod transcript {
        use super::*;

        #[test]
        fn the_last_reply_names_the_model_and_the_tokens_it_was_sent() {
            let mut file = Written::new();

            let found = file.lines(&[
                user("hi"),
                assistant("claude-sonnet-5-5", 1, 2, 3),
                assistant("claude-opus-5-5", 2, 15_655, 149_954),
            ]);

            assert_eq!(found, Some(reply("claude-opus-5-5", 165_611)));
        }

        #[test]
        fn lines_after_the_reply_keep_it() {
            let mut file = Written::new();

            let found = file.lines(&[assistant("claude-opus-5-5", 1, 0, 9), user("tool result")]);

            assert_eq!(found, Some(reply("claude-opus-5-5", 10)));
        }

        #[rstest]
        #[case::subagent(r#"{"isSidechain":true,"type":"assistant","message":{"model":"claude-haiku-4-5","usage":{"input_tokens":5}}}"#)]
        #[case::synthetic(r#"{"type":"assistant","message":{"model":"<synthetic>","usage":{"input_tokens":0}}}"#)]
        #[case::no_usage(r#"{"type":"assistant","message":{"model":"claude-opus-5-5"}}"#)]
        #[case::no_tokens(r#"{"type":"assistant","message":{"model":"claude-opus-5-5","usage":{"input_tokens":0}}}"#)]
        #[case::quoted_in_a_tool_result(
            r#"{"type":"user","message":{"content":"{\"type\":\"assistant\"}"},"toolUseResult":{"type":"assistant"}}"#
        )]
        #[case::broken(r#"{"type":"assistant","message":{"model":"claude-opus-5-5","usa"#)]
        fn some_lines_are_not_a_reply(#[case] line: &str) {
            let mut file = Written::new();

            let found = file.lines(&[assistant("claude-opus-5-5", 1, 0, 9), line.to_string()]);

            assert_eq!(found, Some(reply("claude-opus-5-5", 10)));
        }

        #[test]
        fn compacting_hides_it_until_the_next_reply() {
            let mut file = Written::new();
            file.lines(&[assistant("claude-opus-5-5", 1, 0, 180_000)]);

            let compacted = file.lines(&[COMPACTED.to_string(), user("summary")]);
            let answered = file.lines(&[assistant("claude-opus-5-5", 1, 0, 30_000)]);

            assert_eq!((compacted, answered), (None, Some(reply("claude-opus-5-5", 30_001))));
        }

        #[test]
        fn a_line_still_being_written_waits_for_its_end() {
            let mut file = Written::new();
            file.lines(&[assistant("claude-opus-5-5", 1, 0, 9)]);
            let line = assistant("claude-opus-5-5", 1, 0, 99);
            let (head, tail) = line.split_at(40);

            let half = file.append(head);
            let whole = file.append(&format!("{tail}\n"));

            assert_eq!((half, whole), (Some(reply("claude-opus-5-5", 10)), Some(reply("claude-opus-5-5", 100))));
        }

        #[test]
        fn only_the_last_mebibyte_is_read_at_first() {
            let mut file = Written::new();
            let filler = user(&"x".repeat(1000));
            let lines: Vec<String> = std::iter::once(assistant("claude-opus-4-1", 1, 0, 9))
                .chain(std::iter::repeat_n(filler, 1100))
                .collect();

            assert_eq!(file.lines(&lines), None);
        }

        #[test]
        fn a_reply_within_the_last_mebibyte_is_found() {
            let mut file = Written::new();
            let filler = user(&"x".repeat(1000));
            let lines: Vec<String> = std::iter::repeat_n(filler, 1100)
                .chain(std::iter::once(assistant("claude-opus-5-5", 1, 0, 9)))
                .collect();

            assert_eq!(file.lines(&lines), Some(reply("claude-opus-5-5", 10)));
        }

        #[test]
        fn a_compaction_in_a_large_append_is_not_skipped() {
            let mut file = Written::new();
            file.lines(&[assistant("claude-opus-5-5", 1, 0, 180_000)]);
            let lines: Vec<String> = std::iter::once(COMPACTED.to_string())
                .chain(std::iter::repeat_n(user(&"x".repeat(1000)), 1100))
                .collect();

            assert_eq!(file.lines(&lines), None);
        }

        #[test]
        fn a_file_that_starts_over_is_read_again() {
            let mut file = Written::new();
            file.lines(&[assistant("claude-opus-5-5", 1, 0, 9_999), user("a long question")]);
            std::fs::write(
                file.dir.path().join("session.jsonl"),
                format!("{}\n", assistant("claude-haiku-4-5", 1, 0, 9)),
            )
            .expect("rewrite the transcript");

            file.transcript.update();

            assert_eq!(file.transcript.reply, Some(reply("claude-haiku-4-5", 10)));
        }

        #[test]
        fn a_transcript_that_goes_away_takes_its_reply_with_it() {
            let mut file = Written::new();
            file.lines(&[assistant("claude-opus-5-5", 1, 0, 9)]);
            std::fs::remove_file(file.dir.path().join("session.jsonl")).expect("remove the transcript");

            file.transcript.update();

            assert_eq!(file.transcript.reply, None);
        }

        #[test]
        fn a_missing_file_has_no_reply() {
            let mut transcript = Transcript::new(PathBuf::from("/nonexistent/session.jsonl"));

            transcript.update();

            assert_eq!(transcript.reply, None);
        }
    }

    mod folder {
        use super::*;

        const LONG_ASCII: &str = "-home-ana-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-folder-name-very-long-fold-3mvvc4";

        #[rstest]
        #[case::plain("/home/ubuntu/projects/shop", "-home-ubuntu-projects-shop")]
        #[case::dots_and_dashes(
            "/home/a/.cornercase/worktrees/shop/feat-x",
            "-home-a--cornercase-worktrees-shop-feat-x"
        )]
        #[case::accents_and_emoji("/home/ana/código/🦀 shop", "-home-ana-c-digo----shop")]
        fn is_the_folder_with_every_other_character_as_a_dash(#[case] cwd: &str, #[case] expected: &str) {
            assert_eq!(project_dir(Path::new(cwd)), expected);
        }

        #[test]
        fn a_long_one_is_cut_and_gets_a_hash() {
            let cwd = format!("/home/ana/{}repo", "very-long-folder-name/".repeat(10));

            assert_eq!(project_dir(Path::new(&cwd)), LONG_ASCII);
        }

        #[test]
        fn a_long_one_counts_characters_like_javascript() {
            let cwd = format!("/home/ana/{}/x", "ñ".repeat(195));

            assert_eq!(project_dir(Path::new(&cwd)), format!("-home-ana-{}-ck6jya", "-".repeat(190)));
        }

        #[test]
        fn holds_the_transcript_of_the_session() {
            let path = transcript_path(Path::new("/c"), Path::new("/home/a/shop"), "a01c-9f");

            assert_eq!(path, Some(PathBuf::from("/c/projects/-home-a-shop/a01c-9f.jsonl")));
        }

        #[rstest]
        #[case::empty("")]
        #[case::path("../../etc/passwd")]
        fn a_strange_session_id_has_none(#[case] id: &str) {
            assert_eq!(transcript_path(Path::new("/c"), Path::new("/home/a/shop"), id), None);
        }
    }

    mod model {
        use super::*;

        #[rstest]
        #[case::current("claude-opus-5-5", "Opus 5.5")]
        #[case::dated("claude-sonnet-4-5-20250929", "Sonnet 4.5")]
        #[case::major_only("claude-sonnet-4-20250514", "Sonnet 4")]
        #[case::zero_minor("claude-opus-4-0", "Opus 4")]
        #[case::haiku("claude-haiku-4-5-20251001", "Haiku 4.5")]
        #[case::fable("claude-fable-5-1", "Fable 5.1")]
        #[case::version_first("claude-3-5-sonnet-20241022", "Sonnet 3.5")]
        #[case::bedrock("us.anthropic.claude-opus-4-1-20250805-v1:0", "Opus 4.1")]
        #[case::vertex("claude-opus-4-1@20250805", "Opus 4.1")]
        #[case::another_provider("glm-4.6", "glm-4.6")]
        fn is_named_like_claude_names_it(#[case] id: &str, #[case] expected: &str) {
            assert_eq!(model_name(id), expected);
        }
    }

    mod window {
        use super::*;

        fn limits(model: Option<&str>) -> Limits {
            Limits { model: model.map(str::to_string), ..Limits::default() }
        }

        #[rstest]
        #[case::opus_5_5("claude-opus-5-5", None, LONG_WINDOW)]
        #[case::opus_4_7("claude-opus-4-7", None, LONG_WINDOW)]
        #[case::sonnet_5("claude-sonnet-5", None, LONG_WINDOW)]
        #[case::fable("claude-fable-5-1", None, LONG_WINDOW)]
        #[case::opus_4_6("claude-opus-4-6", None, STANDARD_WINDOW)]
        #[case::sonnet_4_5("claude-sonnet-4-5-20250929", None, STANDARD_WINDOW)]
        #[case::haiku("claude-haiku-4-5-20251001", None, STANDARD_WINDOW)]
        #[case::asked_by_alias("claude-sonnet-4-5-20250929", Some("sonnet[1m]"), LONG_WINDOW)]
        #[case::asked_by_id("claude-sonnet-4-5-20250929", Some("claude-sonnet-4-5[1M]"), LONG_WINDOW)]
        #[case::asked_for_another_model("claude-haiku-4-5-20251001", Some("sonnet[1m]"), STANDARD_WINDOW)]
        #[case::plain_setting("claude-sonnet-4-5-20250929", Some("sonnet"), STANDARD_WINDOW)]
        fn follows_the_model(#[case] model: &str, #[case] setting: Option<&str>, #[case] expected: u64) {
            assert_eq!(limits(setting).window(model, 1000), expected);
        }

        #[test]
        fn more_than_200k_tokens_means_1m() {
            assert_eq!(limits(None).window("claude-sonnet-4-5", 200_001), LONG_WINDOW);
        }

        #[test]
        fn turning_1m_off_keeps_200k() {
            let off = Limits { no_long: true, ..limits(Some("opus[1m]")) };

            assert_eq!(off.window("claude-opus-5-5", 1000), STANDARD_WINDOW);
        }

        #[rstest]
        #[case::unknown_model("glm-4.6", false, 128_000)]
        #[case::claude_model("claude-sonnet-4-5", false, STANDARD_WINDOW)]
        #[case::compacting_off("claude-opus-5-5", true, 128_000)]
        fn a_maximum_counts_for_other_models_or_with_compacting_off(
            #[case] model: &str,
            #[case] no_compact: bool,
            #[case] expected: u64,
        ) {
            let max = Limits { max_tokens: Some(128_000), no_compact, ..Limits::default() };

            assert_eq!(max.window(model, 1000), expected);
        }

        #[rstest]
        #[case::nearly_three_quarters(149_954, STANDARD_WINDOW, 75)]
        #[case::rounds_half_up(1_000, STANDARD_WINDOW, 1)]
        #[case::nothing_much(400, STANDARD_WINDOW, 0)]
        #[case::long(165_611, LONG_WINDOW, 17)]
        #[case::never_above_100(250_000, STANDARD_WINDOW, 100)]
        fn the_percentage_is_rounded_like_claude_does(#[case] tokens: u64, #[case] window: u64, #[case] expected: u16) {
            assert_eq!(percent(tokens, window), expected);
        }
    }

    mod limits {
        use super::*;

        fn env(vars: &[(&str, &str)]) -> HashMap<String, String> {
            vars.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
        }

        #[rstest]
        #[case::flag(&["claude", "--model", "opus[1m]"], &[(MODEL_ENV, "sonnet")], Some("opus[1m]"))]
        #[case::flag_with_equals(&["claude", "--model=haiku"], &[], Some("haiku"))]
        #[case::last_flag(&["claude", "--model", "opus", "--model", "sonnet"], &[], Some("sonnet"))]
        #[case::environment(&["claude"], &[(MODEL_ENV, "sonnet[1m]")], Some("sonnet[1m]"))]
        #[case::empty_environment(&["claude"], &[(MODEL_ENV, " ")], Some("fable"))]
        #[case::settings(&["claude"], &[], Some("fable"))]
        fn the_model_comes_from_the_flag_then_the_environment_then_the_settings(
            #[case] flags: &[&str],
            #[case] vars: &[(&str, &str)],
            #[case] expected: Option<&str>,
        ) {
            let found = Limits::new(&strings(flags), &env(vars), Some("fable".into())).model;

            assert_eq!(found.as_deref(), expected);
        }

        #[rstest]
        #[case::one("1", true)]
        #[case::yes(" YES ", true)]
        #[case::on("on", true)]
        #[case::zero("0", false)]
        #[case::anything_else("please", false)]
        fn switches_take_what_claude_takes(#[case] value: &str, #[case] expected: bool) {
            let found = Limits::new(&[], &env(&[(NO_LONG_ENV, value), (NO_COMPACT_ENV, value)]), None);

            assert_eq!((found.no_long, found.no_compact), (expected, expected));
        }

        #[rstest]
        #[case::number("300000", Some(300_000))]
        #[case::zero("0", None)]
        #[case::text("lots", None)]
        fn the_maximum_must_be_a_positive_number(#[case] value: &str, #[case] expected: Option<u64>) {
            assert_eq!(Limits::new(&[], &env(&[(MAX_TOKENS_ENV, value)]), None).max_tokens, expected);
        }
    }

    struct Setup {
        claude_dir: TempDir,
        project: TempDir,
        sleeper: Sleeper,
    }

    impl Setup {
        fn new(vars: &[(&str, &str)]) -> Self {
            Self { claude_dir: TempDir::new(), project: TempDir::new(), sleeper: Sleeper::with_env(vars) }
        }

        fn claude(&self, args: &[&str]) -> Claude {
            let text = serde_json::json!({ "sessionId": "s1", "cwd": self.project.path() }).to_string();
            let session = Session::parse(&text, self.sleeper.pid());
            Claude { pid: self.sleeper.pid(), args: strings(args), session }
        }

        fn limits(&self, args: &[&str]) -> Limits {
            Limits::read(self.claude_dir.path(), self.project.path(), &self.claude(args))
        }

        fn transcript(&self, lines: &[String]) {
            let path = transcript_path(self.claude_dir.path(), self.project.path(), "s1").expect("a transcript path");
            write_file(&path, &joined(lines));
        }
    }

    mod settings {
        use super::*;

        #[test]
        fn the_project_wins_over_the_user_and_the_local_file_over_both() {
            let s = Setup::new(&[]);
            write_file(&s.claude_dir.path().join("settings.json"), r#"{"model":"haiku"}"#);
            write_file(&s.project.path().join(".claude/settings.json"), r#"{"model":"sonnet"}"#);
            write_file(&s.project.path().join(".claude/settings.local.json"), r#"{"model":"opus[1m]"}"#);

            assert_eq!(s.limits(&["claude"]).model.as_deref(), Some("opus[1m]"));
        }

        #[test]
        fn variables_come_from_claude_and_its_settings() {
            let s = Setup::new(&[(MAX_TOKENS_ENV, "300000"), (NO_LONG_ENV, "0")]);
            write_file(
                &s.claude_dir.path().join("settings.json"),
                r#"{"env":{"CLAUDE_CODE_DISABLE_1M_CONTEXT":"1","DISABLE_COMPACT":true}}"#,
            );

            let found = s.limits(&["claude"]);

            assert_eq!((found.max_tokens, found.no_long, found.no_compact), (Some(300_000), true, false));
        }

        #[test]
        fn a_broken_file_is_skipped() {
            let s = Setup::new(&[(MODEL_ENV, "sonnet[1m]")]);
            write_file(&s.project.path().join(".claude/settings.json"), r#"{"model":"#);

            assert_eq!(s.limits(&["claude"]).model.as_deref(), Some("sonnet[1m]"));
        }
    }

    mod pane {
        use super::*;

        fn answered() -> (Setup, Pane) {
            let s = Setup::new(&[]);
            s.transcript(&[assistant("claude-opus-5-5", 2, 15_655, 149_954)]);
            let mut pane = Pane::default();
            pane.update(Some(s.claude_dir.path()), Some(&s.claude(&["claude"])));
            (s, pane)
        }

        #[test]
        fn shows_the_model_and_the_share_of_its_window() {
            let (_s, pane) = answered();

            assert_eq!(pane.context(), Some(&Context { model: "Opus 5.5".into(), percent: Some(17) }));
        }

        #[test]
        fn a_1m_setting_counts_for_an_older_model() {
            let s = Setup::new(&[]);
            s.transcript(&[assistant("claude-sonnet-4-5-20250929", 0, 0, 100_000)]);
            let mut pane = Pane::default();

            pane.update(Some(s.claude_dir.path()), Some(&s.claude(&["claude", "--model", "sonnet[1m]"])));

            assert_eq!(pane.context().and_then(|c| c.percent), Some(10));
        }

        #[test]
        fn a_new_session_starts_empty() {
            let (s, mut pane) = answered();
            let mut cleared = s.claude(&["claude"]);
            cleared.session.as_mut().expect("a session").id = Some("s2".into());

            pane.update(Some(s.claude_dir.path()), Some(&cleared));

            assert_eq!(pane.context(), None);
        }

        #[test]
        fn goes_away_with_claude() {
            let (s, mut pane) = answered();

            pane.update(Some(s.claude_dir.path()), None);

            assert_eq!(pane.context(), None);
        }
    }
}
