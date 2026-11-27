use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::Record;
use crate::agents::{copilot, grok, vibe};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stamp(Vec<(u64, Option<SystemTime>)>);

impl Record {
    pub(crate) fn stamp(&self) -> Stamp {
        Stamp(self.files().iter().map(|file| stat(file)).collect())
    }

    fn files(&self) -> Vec<PathBuf> {
        match self {
            Self::Claude(path) | Self::Codex(path) | Self::Gemini(path) => vec![path.clone()],
            Self::Opencode { database: path, .. } | Self::Cursor(path) => vec![path.clone(), wal(path)],
            Self::Grok(dir) => vec![grok::session::chat(dir)],
            Self::Copilot(dir) => vec![copilot::message::events(dir)],
            Self::Vibe(dir) => vibe::message::journals(dir),
        }
    }
}

fn stat(file: &Path) -> (u64, Option<SystemTime>) {
    std::fs::metadata(file).map_or((0, None), |metadata| (metadata.len(), metadata.modified().ok()))
}

fn wal(database: &Path) -> PathBuf {
    let mut name = OsString::from(database.as_os_str());
    name.push("-wal");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests;
