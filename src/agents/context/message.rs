use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agents::claude::message::claude;
use crate::agents::codex::message::codex;
use crate::agents::gemini::session as gemini;
use crate::agents::opencode::database as opencode;
use crate::error::{Error, Result};

const CHUNK: u64 = 256 * 1024;
const MAX_BACK: u64 = 64 * 1024 * 1024;

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

pub(in crate::agents) fn joined(texts: impl IntoIterator<Item = String>) -> Option<String> {
    let texts: Vec<String> = texts.into_iter().map(|text| text.trim().to_string()).filter(|t| !t.is_empty()).collect();
    (!texts.is_empty()).then(|| texts.join("\n\n"))
}

pub(in crate::agents) struct Backwards {
    file: File,
    at: u64,
    left: u64,
    carry: Vec<u8>,
    lines: Vec<Vec<u8>>,
    partial: bool,
}

impl Backwards {
    pub(in crate::agents) fn open(path: &Path) -> io::Result<Self> {
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

pub(in crate::agents) fn unreadable(path: &Path, e: &io::Error) -> Error {
    Error::Record { path: path.to_path_buf(), cause: e.to_string() }
}

pub(in crate::agents) fn lines(path: &Path) -> Result<Option<Backwards>> {
    match Backwards::open(path) {
        Ok(lines) => Ok(Some(lines)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(unreadable(path, &e)),
    }
}

pub(in crate::agents) fn texts(content: Option<&Value>, kind: &str) -> Vec<String> {
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

#[cfg(test)]
pub(in crate::agents) mod tests;
