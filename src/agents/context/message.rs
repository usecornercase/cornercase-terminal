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
pub(crate) struct Said {
    pub(crate) text: String,
    pub(crate) written: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record {
    Claude(PathBuf),
    Codex(PathBuf),
    Gemini(PathBuf),
    Opencode { database: PathBuf, session: String },
}

impl Record {
    pub(crate) fn last_message(&self) -> Result<Option<Said>> {
        match self {
            Self::Claude(path) => claude(path),
            Self::Codex(path) => codex(path),
            Self::Gemini(path) => gemini::last_message(path),
            Self::Opencode { database, session } => opencode::last_message(database, session),
        }
    }
}

pub(super) fn joined(texts: impl IntoIterator<Item = String>) -> Option<String> {
    let texts: Vec<String> = texts.into_iter().map(|text| text.trim().to_string()).filter(|t| !t.is_empty()).collect();
    (!texts.is_empty()).then(|| texts.join("\n\n"))
}

pub(super) struct Backwards {
    file: File,
    at: u64,
    left: u64,
    carry: Vec<u8>,
    lines: Vec<Vec<u8>>,
    partial: bool,
}

impl Backwards {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
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
mod tests;
