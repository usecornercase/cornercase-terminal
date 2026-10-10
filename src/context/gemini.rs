use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use super::discovery::{folder, home, named};
use super::jsonl::{Tail, head};
use super::message::{self, Backwards, Said};
use super::{Context, percent};
use crate::error::{Error, Result};
use crate::log::Stamp;
use crate::process;

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
pub(super) struct Session {
    pub path: PathBuf,
    id: String,
    tail: Tail,
    replies: Vec<Reply>,
    footer: Option<String>,
    picked: Option<(String, SystemTime)>,
    pub context: Option<Context>,
    pub prompted: Option<SystemTime>,
    used: bool,
    model: Option<String>,
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
        }
    }

    pub fn id(&self) -> Option<&str> {
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
        if let Some(set) = entry.get("$set") {
            if let Some(messages) = set.get("messages") {
                self.replies.clear();
                for message in messages.as_array().into_iter().flatten() {
                    self.message(message);
                }
            }
        } else {
            self.message(entry);
        }
    }

    fn message(&mut self, entry: &Value) {
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

pub(super) fn look(pid: i32, since: SystemTime, previous: Option<Session>, footer: Option<String>) -> Option<Session> {
    let family: Vec<i32> = std::iter::once(pid).chain(process::descendants(pid)).collect();
    let holder = family.iter().rev().copied().find(|&pid| named(&process::args(pid), NAME)).unwrap_or(pid);
    let dir = home(holder, "GEMINI_CLI_HOME", ".gemini", ".gemini")?;
    let cwd = folder(holder)?;
    if process::all().into_iter().any(|other| {
        !family.contains(&other)
            && named(&process::args(other), NAME)
            && folder(other).as_ref() == Some(&cwd)
            && home(other, "GEMINI_CLI_HOME", ".gemini", ".gemini").as_ref() == Some(&dir)
            && !process::descendants(other).contains(&pid)
    }) {
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
    let (path, id) = std::fs::read_dir(project.join("chats"))
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
        .map(|(_, path, id)| (path, id))?;
    let mut session = previous.filter(|s| s.path == path && s.id == id).unwrap_or_else(|| Session::new(path, id));
    session.update(footer)?;
    Some(session)
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

pub(super) fn last_message(path: &Path) -> Result<Option<Said>> {
    let failed = |e: std::io::Error| Error::Record { path: path.to_path_buf(), cause: e.to_string() };
    let lines = match Backwards::open(path) {
        Ok(lines) => lines,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed(e)),
    };
    let mut seen = HashSet::new();
    for line in lines {
        let line = line.map_err(failed)?;
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else { continue };
        if let Some(messages) = entry["$set"].get("messages") {
            for entry in messages.as_array().into_iter().flatten().rev() {
                if let Some(said) = said(entry, &mut seen) {
                    return Ok(Some(said));
                }
            }
            return Ok(None);
        }
        if let Some(said) = said(&entry, &mut seen) {
            return Ok(Some(said));
        }
    }
    Ok(None)
}

fn said(entry: &Value, seen: &mut HashSet<String>) -> Option<Said> {
    if !seen.insert(entry["id"].as_str()?.to_string()) || entry["type"] != "gemini" || entry["model"].as_str().is_none()
    {
        return None;
    }
    Some(Said { text: text(&entry["content"])?, written: entry["timestamp"].as_str().map(str::to_string) })
}

pub(super) fn footer_model(screen: &str) -> Option<String> {
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
mod tests {
    use std::io::Write;
    use std::time::UNIX_EPOCH;

    use rstest::rstest;

    use super::*;
    use crate::test_util::{FakeGemini, TempDir};

    const INITIAL: &str = include_str!("../../tests/fixtures/gemini/0.63.0/initial.jsonl");
    const PROMPT: &str = include_str!("../../tests/fixtures/gemini/0.63.0/prompt.jsonl");
    const REPLY: &str = include_str!("../../tests/fixtures/gemini/0.63.0/reply.jsonl");
    const COMPACTED: &str = include_str!("../../tests/fixtures/gemini/0.63.0/compacted.jsonl");

    fn written(text: &str) -> (TempDir, Session) {
        let dir = TempDir::new();
        let path = dir.path().join("session.jsonl");
        std::fs::write(&path, format!("{INITIAL}{text}")).expect("write session");
        let mut session = Session::new(path, FakeGemini::ID.into());
        session.update(None).expect("read session");
        (dir, session)
    }

    fn append(session: &mut Session, text: &str) {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&session.path)
            .expect("open session")
            .write_all(text.as_bytes())
            .expect("append records");
        session.update(None).expect("update session");
    }

    #[test]
    fn the_initial_context_is_neither_a_prompt_nor_a_conversation_to_resume() {
        let (_dir, session) = written("");
        assert_eq!((session.prompted, session.id(), session.context.as_ref()), (None, None, None));
    }

    #[test]
    fn real_prompts_and_replies_supply_times_model_usage_and_resume() {
        let (_dir, session) = written(&format!("{PROMPT}{REPLY}"));
        let prompt: Value = serde_json::from_str(PROMPT).expect("prompt");
        assert_eq!(session.prompted, prompt["timestamp"].as_str().and_then(Stamp::parse));
        assert_eq!(session.id(), Some(FakeGemini::ID));
        assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
        assert_eq!(last_message(&session.path).expect("message").map(|s| s.text), Some("OK".into()));
    }

    #[test]
    fn the_latest_update_of_a_message_wins() {
        let (_dir, mut session) = written(REPLY);
        let mut changed: Value = serde_json::from_str(REPLY).expect("reply");
        changed["tokens"]["input"] = Value::from(524_288);
        changed["content"] = Value::from("Updated answer");
        append(&mut session, &format!("{changed}\n"));
        assert_eq!(session.replies.len(), 1);
        assert_eq!(session.context.as_ref().and_then(|c| c.percent), Some(50));
        assert_eq!(last_message(&session.path).expect("message").map(|s| s.text), Some("Updated answer".into()));
        changed["content"] = Value::from("");
        append(&mut session, &format!("{changed}\n"));
        assert_eq!(last_message(&session.path).expect("no old update"), None);
    }

    #[test]
    fn compression_replaces_the_history_and_clears_old_usage_without_confirming_a_prompt() {
        let (_dir, mut session) = written(&format!("{PROMPT}{REPLY}"));
        let before = session.prompted;
        append(&mut session, COMPACTED);
        assert_eq!(session.prompted, before);
        assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: None }));
        assert_eq!(last_message(&session.path).expect("no hidden summary"), None);
        assert_eq!(session.id(), Some(FakeGemini::ID));
    }

    #[test]
    fn a_compressed_history_is_resumable_when_discovered_without_its_original_prompts() {
        let (_dir, session) = written(COMPACTED);
        assert_eq!(session.id(), Some(FakeGemini::ID));
        assert_eq!((session.prompted, session.context.as_ref()), (None, None));
        assert_eq!(last_message(&session.path).expect("no hidden summary"), None);
    }

    #[test]
    fn a_resumed_history_keeps_message_times_and_omits_thought_parts() {
        let (_dir, session) = written(include_str!("../../tests/fixtures/gemini/0.63.0/resumed.jsonl"));
        let said = last_message(&session.path).expect("read resumed reply").expect("resumed reply");
        assert!(said.text.starts_with("If there were finitely many primes"));
        assert!(!said.text.contains("Formulating Prime Proof"));
        assert_eq!(said.written.as_deref(), Some("2026-10-10T13:10:47.566Z"));
        assert_eq!(session.prompted, Stamp::parse("2026-10-10T13:10:42.278Z"));
        assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
    }

    #[rstest]
    #[case::tool_result(serde_json::json!([{"functionResponse":{"name":"shell","response":{"output":"OK"}}}]), false)]
    #[case::setup(serde_json::json!([{"text":"<session_context>setup"}]), false)]
    #[case::hook(serde_json::json!([{"text":" \n <hook_context>setup"}]), false)]
    #[case::summary(serde_json::json!([{"text":"<state_snapshot>summary"}]), false)]
    #[case::scratchpad(serde_json::json!([{"text":"<scratchpad>summary"}]), false)]
    #[case::slash(serde_json::json!([{"text":" \n /clear"}]), false)]
    #[case::help(serde_json::json!(" \n ?help"), false)]
    #[case::empty(serde_json::json!(" \n \t "), false)]
    #[case::real(serde_json::json!([{"text":"fix the hook / command"}]), true)]
    fn user_content_controls_confirmation_and_resumability(#[case] content: Value, #[case] expected: bool) {
        let entry = serde_json::json!({
            "id": "candidate", "type": "user", "content": content, "timestamp": "2026-10-10T13:00:00.000Z"
        });
        let (_dir, session) = written(&format!("{entry}\n"));
        assert_eq!(session.prompted.is_some(), expected);
        assert_eq!(session.id().is_some(), expected);
    }

    #[test]
    fn a_model_switch_shows_the_id_without_old_usage_until_it_replies() {
        let (_dir, mut session) = written(REPLY);
        append(
            &mut session,
            "{\"id\":\"switch\",\"type\":\"info\",\"timestamp\":\"2026-10-10T13:00:00.000Z\",\
             \"content\":\"Model set to gemma-4-31b-it\"}\n",
        );
        assert_eq!(session.context, Some(Context { model: "gemma-4-31b-it".into(), percent: None }));
        let mut reply: Value = serde_json::from_str(REPLY).expect("reply");
        reply["id"] = Value::from("new-reply");
        reply["model"] = Value::from("gemma-4-31b-it");
        reply["timestamp"] = Value::from("2026-10-10T13:01:00.000Z");
        reply["tokens"]["input"] = Value::from(128_000);
        append(&mut session, &format!("{reply}\n"));
        assert_eq!(session.context.as_ref().and_then(|c| c.percent), Some(50));
    }

    #[test]
    fn an_unsaved_picker_selection_clears_usage_and_a_fresh_reply_restores_it() {
        let (_dir, mut session) = written(REPLY);
        session.update(Some("gemini-3.5-flash-lite".into())).expect("read the selected model");
        assert_eq!(session.context, Some(Context { model: "gemini-3.5-flash-lite".into(), percent: None }));
        session.update(Some("auto".into())).expect("return to auto");
        assert_eq!(session.context, Some(Context { model: "auto".into(), percent: None }));
        let mut reply: Value = serde_json::from_str(REPLY).expect("reply");
        reply["id"] = Value::from("fresh-reply");
        reply["timestamp"] = Value::from(Stamp(SystemTime::now() + std::time::Duration::from_secs(1)).to_string());
        append(&mut session, &format!("{reply}\n"));
        assert_eq!(session.context, Some(Context { model: "gemini-3.8-flash".into(), percent: Some(1) }));
        std::fs::write(&session.path, INITIAL).expect("clear the history");
        session.update(None).expect("read cleared history");
        assert_eq!(session.context, None);
    }

    #[test]
    fn the_current_footer_takes_precedence_over_historical_model_settings() {
        let (_dir, session) = written(&format!(
            "{REPLY}{{\"id\":\"setting\",\"type\":\"info\",\"content\":\"Model set to auto\",\
             \"timestamp\":\"2026-10-10T13:00:00.000Z\"}}\n"
        ));
        let mut first = Session::new(session.path, FakeGemini::ID.into());
        first.update(Some("gemini-3.5-flash-lite".into())).expect("first discovery with the current footer");
        assert_eq!(first.context, Some(Context { model: "gemini-3.5-flash-lite".into(), percent: None }));
    }

    #[test]
    fn only_complete_lines_count_and_a_truncation_starts_over() {
        let (_dir, mut session) = written(PROMPT);
        let partial = REPLY.trim_end();
        append(&mut session, partial);
        assert_eq!(session.context, None);
        append(&mut session, "\n");
        assert!(session.context.is_some());
        std::fs::write(&session.path, INITIAL).expect("truncate session");
        session.update(None).expect("read truncated file");
        assert_eq!((session.prompted, session.id(), session.context.as_ref()), (None, None, None));
    }

    #[test]
    fn replacement_and_deletion_do_not_keep_old_context() {
        let (_dir, mut session) = written(REPLY);
        let next = session.path.with_extension("next");
        std::fs::write(&next, INITIAL).expect("new inode");
        std::fs::rename(next, &session.path).expect("replace session");
        session.update(None).expect("read replacement");
        assert_eq!(session.context, None);
        std::fs::remove_file(&session.path).expect("delete file");
        assert!(session.update(None).is_none());
    }

    #[test]
    fn discovery_uses_one_home_and_folder_and_rejects_stale_or_subagent_sessions() {
        let project = TempDir::new();
        let fake = FakeGemini::new(project.path());
        let first = fake.start(project.path());
        fake.append(REPLY);
        assert!(look(first.pid(), UNIX_EPOCH, None, None).is_some());
        let second = fake.start(project.path());
        assert!(look(first.pid(), UNIX_EPOCH, None, None).is_none());
        drop(second);
        assert!(look(first.pid(), SystemTime::now(), None, None).is_none());
        let other_home = FakeGemini::new(project.path());
        let _other = other_home.start(project.path());
        assert!(look(first.pid(), UNIX_EPOCH, None, None).is_some());
        std::fs::write(&fake.session, INITIAL.replace("\"main\"", "\"subagent\"")).expect("subagent header");
        assert!(look(first.pid(), UNIX_EPOCH, None, None).is_none());
    }

    #[rstest]
    #[case::model(
        "workspace (/directory)          sandbox          /model\n\
         /tmp/project                 no sandbox    gemini-3.8-flash",
        Some("gemini-3.8-flash")
    )]
    #[case::automatic("workspace (/directory)  sandbox  /model\n/tmp/project      no sandbox      Auto", Some("auto"))]
    #[case::context_only("/model  context\nAuto  1% used", Some("auto"))]
    #[case::quota(
        "workspace (/directory)  sandbox  /model  context  quota\n\
         /tmp/project  no sandbox  gemini-3.8-flash  1% used  95% left",
        Some("gemini-3.8-flash")
    )]
    #[case::hidden_workspace(
        "sandbox  /model  context  quota\nno sandbox  gemma-4-31b-it  50% used  95% left",
        Some("gemma-4-31b-it")
    )]
    #[case::branch_with_custom_model(
        "workspace (/directory)  branch  /model\n/tmp/project  gemini-foo  custom-model",
        None
    )]
    #[case::mismatched_cells("sandbox  /model  context\nno sandbox  gemini-3.8-flash", None)]
    #[case::truncated("workspace (/directory)  sandbox  /model\n/tmp/project  no sandbox  gemini-3…", None)]
    #[case::dialog("Select Model\n● gemini-3.8-flash", None)]
    #[case::outside_footer("/model  context\ngemini-3.8-flash  1% used\none\ntwo\nthree\nfour\nfive\nsix", None)]
    fn the_labelled_footer_only_supplies_complete_model_ids(#[case] screen: &str, #[case] expected: Option<&str>) {
        assert_eq!(footer_model(screen).as_deref(), expected);
    }
}
