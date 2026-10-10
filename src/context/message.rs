use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::{ASSISTANT, SYNTHETIC, gemini, opencode};
use crate::error::{Error, Result};

const CHUNK: u64 = 256 * 1024;
const MAX_BACK: u64 = 64 * 1024 * 1024;
const CODEX_ASSISTANT: &str = r#""role":"assistant""#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub text: String,
    pub written: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    Claude(PathBuf),
    Codex(PathBuf),
    Gemini(PathBuf),
    Opencode { database: PathBuf, session: String },
}

impl Record {
    pub fn last_message(&self) -> Result<Option<Said>> {
        match self {
            Self::Claude(path) => claude(path),
            Self::Codex(path) => codex(path),
            Self::Gemini(path) => gemini::last_message(path),
            Self::Opencode { database, session } => opencode::last_message(database, session),
        }
    }
}

pub fn joined(texts: impl IntoIterator<Item = String>) -> Option<String> {
    let texts: Vec<String> = texts.into_iter().map(|text| text.trim().to_string()).filter(|t| !t.is_empty()).collect();
    (!texts.is_empty()).then(|| texts.join("\n\n"))
}

pub struct Backwards {
    file: File,
    at: u64,
    left: u64,
    carry: Vec<u8>,
    lines: Vec<Vec<u8>>,
    partial: bool,
}

impl Backwards {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let at = file.metadata()?.len();
        Ok(Self { file, at, left: MAX_BACK, carry: Vec::new(), lines: Vec::new(), partial: true })
    }

    fn read(&mut self) -> io::Result<()> {
        let size = CHUNK.min(self.at).min(self.left);
        self.at -= size;
        self.left -= size;
        let mut bytes = vec![0; usize::try_from(size).unwrap_or_default()];
        self.file.seek(SeekFrom::Start(self.at))?;
        self.file.read_exact(&mut bytes)?;
        bytes.append(&mut self.carry);
        if self.partial {
            let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else { return Ok(()) };
            bytes.truncate(end);
            self.partial = false;
        }
        let mut lines = bytes.split(|b| *b == b'\n');
        if self.at > 0 {
            self.carry = lines.next().map(<[u8]>::to_vec).unwrap_or_default();
        }
        self.lines.extend(lines.filter(|line| !line.is_empty()).map(<[u8]>::to_vec));
        Ok(())
    }
}

impl Iterator for Backwards {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(line) = self.lines.pop() {
                return Some(Ok(line));
            }
            if self.at == 0 || self.left == 0 {
                return None;
            }
            if let Err(e) = self.read() {
                self.at = 0;
                return Some(Err(e));
            }
        }
    }
}

fn unreadable(path: &Path, e: &io::Error) -> Error {
    Error::Record { path: path.to_path_buf(), cause: e.to_string() }
}

fn lines(path: &Path) -> Result<Option<Backwards>> {
    match Backwards::open(path) {
        Ok(lines) => Ok(Some(lines)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(unreadable(path, &e)),
    }
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "isSidechain")]
    sidechain: Option<bool>,
    #[serde(rename = "isApiErrorMessage")]
    api_error: Option<bool>,
    timestamp: Option<String>,
    message: Option<Body>,
}

#[derive(Deserialize)]
struct Body {
    id: Option<String>,
    model: Option<String>,
    content: Option<Value>,
}

struct Reply {
    id: Option<String>,
    texts: Vec<String>,
    written: Option<String>,
}

impl Reply {
    fn claude(line: &[u8]) -> Option<Self> {
        let line = std::str::from_utf8(line).ok().filter(|line| line.contains(ASSISTANT))?;
        let entry: Entry = serde_json::from_str(line).ok()?;
        let body = entry.message.filter(|_| entry.kind.as_deref() == Some("assistant"))?;
        let synthetic = body.model.as_deref() == Some(SYNTHETIC) && entry.api_error != Some(true);
        if entry.sidechain == Some(true) || synthetic {
            return None;
        }
        Some(Self { id: body.id, texts: texts(body.content.as_ref(), "text"), written: entry.timestamp })
    }

    fn message(self) -> Option<Said> {
        Some(Said { text: joined(self.texts)?, written: self.written })
    }
}

fn texts(content: Option<&Value>, kind: &str) -> Vec<String> {
    match content {
        Some(Value::String(text)) => vec![text.clone()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some(kind))
            .filter_map(|block| block.get("text")?.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn claude(path: &Path) -> Result<Option<Said>> {
    let Some(lines) = lines(path)? else { return Ok(None) };
    let mut newest: Option<Reply> = None;
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        let Some(reply) = Reply::claude(&line) else { continue };
        match &mut newest {
            None if joined(reply.texts.iter().cloned()).is_some() => newest = Some(reply),
            None => {}
            Some(found) if found.id.is_some() && found.id == reply.id => {
                found.texts.splice(0..0, reply.texts);
            }
            Some(_) => break,
        }
    }
    Ok(newest.and_then(Reply::message))
}

fn codex(path: &Path) -> Result<Option<Said>> {
    let Some(lines) = lines(path)? else { return Ok(None) };
    for line in lines {
        let line = line.map_err(|e| unreadable(path, &e))?;
        if let Some(message) = codex_message(&line) {
            return Ok(Some(message));
        }
    }
    Ok(None)
}

fn codex_message(line: &[u8]) -> Option<Said> {
    let line = std::str::from_utf8(line).ok().filter(|line| line.contains(CODEX_ASSISTANT))?;
    let entry: Value = serde_json::from_str(line).ok()?;
    let payload = entry.get("payload")?;
    let reply = entry.get("type")?.as_str()? == "response_item"
        && payload.get("type")?.as_str()? == "message"
        && payload.get("role")?.as_str()? == "assistant";
    let text = joined(texts(payload.get("content").filter(|_| reply), "output_text"))?;
    Some(Said { text, written: entry.get("timestamp").and_then(Value::as_str).map(str::to_string) })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rstest::rstest;

    use super::*;
    use crate::test_util::TempDir;

    struct Written {
        _dir: TempDir,
        path: PathBuf,
    }

    impl Written {
        fn new(text: &str) -> Self {
            let dir = TempDir::new();
            let path = dir.path().join("record.jsonl");
            std::fs::write(&path, text).expect("write the record");
            Self { _dir: dir, path }
        }

        fn lines(&self) -> Vec<String> {
            let lines = Backwards::open(&self.path).expect("open the record");
            lines.map(|line| String::from_utf8(line.expect("a line")).expect("text")).collect()
        }

        fn append(&self, text: &str) {
            let mut file = std::fs::OpenOptions::new().append(true).open(&self.path).expect("open the record");
            file.write_all(text.as_bytes()).expect("append to the record");
        }
    }

    fn said(id: &str, kind: &str, text: &str) -> String {
        let block = serde_json::json!({ "type": kind, kind: text });
        serde_json::json!({
            "type": "assistant",
            "isSidechain": false,
            "timestamp": format!("2026-10-08T19:56:{:02}.241Z", text.len() % 60),
            "message": { "id": id, "model": "claude-opus-5-5", "role": "assistant", "content": [block] },
        })
        .to_string()
    }

    fn user(text: &str) -> String {
        serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
    }

    fn joined_lines(lines: &[String]) -> String {
        lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect()
    }

    fn last_claude(lines: &[String]) -> Option<String> {
        let file = Written::new(&joined_lines(lines));
        claude(&file.path).expect("read the transcript").map(|m| m.text)
    }

    mod backwards {
        use super::*;

        #[test]
        fn gives_the_newest_line_first() {
            let file = Written::new("one\ntwo\n\nthree\n");

            assert_eq!(file.lines(), ["three", "two", "one"]);
        }

        #[test]
        fn leaves_out_a_line_still_being_written() {
            let file = Written::new("one\ntwo\n");
            file.append("{\"half");

            assert_eq!(file.lines(), ["two", "one"]);
        }

        #[test]
        fn joins_lines_cut_by_its_chunks() {
            let long: Vec<String> = (0..5).map(|n| format!("{n}{}", "x".repeat(200_000))).collect();
            let file = Written::new(&joined_lines(&long));

            let read = file.lines();

            assert_eq!(read, long.iter().rev().cloned().collect::<Vec<_>>());
        }

        #[test]
        fn a_file_of_one_unfinished_line_has_none() {
            let file = Written::new(&"x".repeat(600_000));

            assert_eq!(file.lines(), Vec::<String>::new());
        }

        #[test]
        fn stops_after_its_budget() {
            let line = "y".repeat(1024 * 1024 - 1);
            let file = Written::new(&format!("first\n{}", format!("{line}\n").repeat(64)));

            let read = file.lines();

            assert_eq!((read.len(), read.contains(&"first".to_string())), (63, false));
        }
    }

    mod claude {
        use super::*;

        #[test]
        fn the_last_message_is_the_newest_text_of_the_conversation() {
            let found = last_claude(&[
                user("fix it"),
                said("m1", "text", "Looking."),
                said("m1", "tool_use", "{}"),
                user("tool result"),
                said("m2", "thinking", "hmm"),
                said("m2", "text", "Fixed the login form."),
                said("m3", "thinking", "anything else?"),
            ]);

            assert_eq!(found.as_deref(), Some("Fixed the login form."));
        }

        #[test]
        fn the_text_blocks_of_one_message_come_together_in_order() {
            let found = last_claude(&[
                said("m1", "text", "Old."),
                said("m2", "text", "First part."),
                said("m2", "thinking", "hmm"),
                said("m2", "text", "Second part."),
            ]);

            assert_eq!(found.as_deref(), Some("First part.\n\nSecond part."));
        }

        #[test]
        fn it_carries_when_it_was_written() {
            let file = Written::new(&joined_lines(&[said("m1", "text", "Done.")]));

            let found = claude(&file.path).expect("read the transcript");

            assert_eq!(found, Some(Said { text: "Done.".into(), written: Some("2026-10-08T19:56:05.241Z".into()) }));
        }

        #[rstest]
        #[case::a_subagent(
            r#"{"type":"assistant","isSidechain":true,"message":{"id":"s","model":"claude-haiku-4-5","content":[{"type":"text","text":"Subagent."}]}}"#
        )]
        #[case::nothing_to_say(
            r#"{"type":"assistant","message":{"id":"x","model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]},"isApiErrorMessage":false}"#
        )]
        #[case::blank(r#"{"type":"assistant","message":{"id":"b","model":"claude-opus-5-5","content":[{"type":"text","text":"\n\n"}]}}"#)]
        #[case::a_user_quoting_one(r#"{"type":"user","message":{"content":"{\"type\":\"assistant\"}"}}"#)]
        #[case::broken(r#"{"type":"assistant","message":{"id":"z","content":[{"type":"te"#)]
        fn some_lines_are_not_what_it_said(#[case] line: &str) {
            assert_eq!(last_claude(&[said("m1", "text", "Done."), line.to_string()]).as_deref(), Some("Done."));
        }

        #[test]
        fn an_api_error_is_what_it_said_last() {
            let error = r#"{"type":"assistant","message":{"id":"e","model":"<synthetic>","content":[{"type":"text","text":"API Error: 529 Overloaded"}]},"isApiErrorMessage":true}"#;

            assert_eq!(
                last_claude(&[said("m1", "text", "Done."), error.to_string()]).as_deref(),
                Some("API Error: 529 Overloaded")
            );
        }

        #[test]
        fn a_message_older_than_a_mebibyte_of_tool_output_is_found() {
            let output = user(&"x".repeat(1024 * 1024));

            let found = last_claude(&[said("m1", "text", "Read it all."), output.clone(), output]);

            assert_eq!(found.as_deref(), Some("Read it all."));
        }

        #[test]
        fn a_transcript_not_written_yet_has_no_message() {
            let dir = TempDir::new();

            assert_eq!(claude(&dir.path().join("none.jsonl")).expect("no error"), None);
        }

        #[test]
        fn a_transcript_without_a_reply_has_no_message() {
            assert_eq!(last_claude(&[user("hello"), said("m1", "thinking", "hmm")]), None);
        }

        #[test]
        fn a_transcript_that_cannot_be_read_says_why() {
            let dir = TempDir::new();

            let error = claude(dir.path()).expect_err("a folder is no transcript");

            assert!(error.to_string().starts_with(&format!("cannot read `{}`", dir.path().display())), "{error}");
        }
    }

    mod codex {
        use super::*;

        const CONTEXT: &str = include_str!("../../tests/fixtures/codex/0.160.0/context.jsonl");
        const REPLY: &str = include_str!("../../tests/fixtures/codex/0.160.0/reply.jsonl");

        fn last_codex(text: &str) -> Option<Said> {
            codex(&Written::new(text).path).expect("read the rollout")
        }

        #[test]
        fn the_last_message_is_the_newest_reply_of_the_thread() {
            let found = last_codex(&format!("{CONTEXT}{REPLY}"));

            assert_eq!(
                found,
                Some(Said {
                    text: "The login form now checks the password.\n\nTests pass.".into(),
                    written: Some("2026-10-05T12:00:37.000Z".into())
                })
            );
        }

        #[test]
        fn a_progress_note_counts_until_a_newer_reply() {
            let note = REPLY.lines().find(|line| line.contains("commentary")).expect("a progress note");

            assert_eq!(
                last_codex(&format!("{CONTEXT}{note}\n")).map(|m| m.text).as_deref(),
                Some("Checking the form first.")
            );
        }

        #[test]
        fn a_rollout_without_a_reply_has_no_message() {
            let user = REPLY.lines().find(|line| line.contains(r#""role":"user""#)).expect("a user message");

            assert_eq!(last_codex(&format!("{CONTEXT}{user}\n")), None);
        }
    }
}
