use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Record;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Pointer {
    Claude(PathBuf),
    Codex(PathBuf),
    Gemini(PathBuf),
    Opencode { database: PathBuf, session: String },
    Grok(PathBuf),
    Cursor(PathBuf),
    Copilot(PathBuf),
    Vibe(PathBuf),
}

impl Pointer {
    pub(crate) fn of(record: &Record) -> Self {
        match record {
            Record::Claude(path) => Self::Claude(path.clone()),
            Record::Codex(path) => Self::Codex(path.clone()),
            Record::Gemini(path) => Self::Gemini(path.clone()),
            Record::Opencode { database, session } => {
                Self::Opencode { database: database.clone(), session: session.clone() }
            }
            Record::Grok(path) => Self::Grok(path.clone()),
            Record::Cursor(path) => Self::Cursor(path.clone()),
            Record::Copilot(path) => Self::Copilot(path.clone()),
            Record::Vibe(path) => Self::Vibe(path.clone()),
        }
    }

    pub(crate) fn record(&self) -> Record {
        match self {
            Self::Claude(path) => Record::Claude(path.clone()),
            Self::Codex(path) => Record::Codex(path.clone()),
            Self::Gemini(path) => Record::Gemini(path.clone()),
            Self::Opencode { database, session } => {
                Record::Opencode { database: database.clone(), session: session.clone() }
            }
            Self::Grok(path) => Record::Grok(path.clone()),
            Self::Cursor(path) => Record::Cursor(path.clone()),
            Self::Copilot(path) => Record::Copilot(path.clone()),
            Self::Vibe(path) => Record::Vibe(path.clone()),
        }
    }

    pub(crate) fn file(&self) -> &Path {
        match self {
            Self::Claude(path)
            | Self::Codex(path)
            | Self::Gemini(path)
            | Self::Grok(path)
            | Self::Cursor(path)
            | Self::Copilot(path)
            | Self::Vibe(path) => path,
            Self::Opencode { database, .. } => database,
        }
    }

    pub(crate) fn readable(&self) -> bool {
        self.file().exists()
    }

    pub(crate) fn path(&self) -> String {
        match self {
            Self::Claude(path)
            | Self::Codex(path)
            | Self::Gemini(path)
            | Self::Grok(path)
            | Self::Cursor(path)
            | Self::Copilot(path)
            | Self::Vibe(path) => path.display().to_string(),
            Self::Opencode { database, session } => format!("{}#{session}", database.display()),
        }
    }
}

#[cfg(test)]
mod tests;
