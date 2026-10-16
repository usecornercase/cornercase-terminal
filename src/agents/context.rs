use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::agents::activity::Activity;
use crate::agents::{self, Known, claude, codex, cursor, gemini, grok, opencode};
use crate::error::Result;

pub(in crate::agents) mod counting;
pub(in crate::agents) mod discovery;
pub(in crate::agents) mod jsonl;
pub(in crate::agents) mod message;

pub(crate) use message::{Role, Said, Spoken};

use counting::Ledger;

const TAIL: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) model: String,
    pub(crate) percent: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record {
    Claude(PathBuf),
    Codex(PathBuf),
    Gemini(PathBuf),
    Opencode { database: PathBuf, session: String },
    Grok(PathBuf),
    Cursor(PathBuf),
}

impl Record {
    pub(crate) fn last_message(&self) -> Result<Option<Said>> {
        match self {
            Self::Claude(path) => claude::message::claude(path),
            Self::Codex(path) => codex::message::codex(path),
            Self::Gemini(path) => gemini::session::last_message(path),
            Self::Opencode { database, session } => opencode::database::last_message(database, session),
            Self::Grok(path) => grok::session::last_message(path),
            Self::Cursor(path) => cursor::store::last_message(path),
        }
    }

    pub(crate) fn said(&self, each: &mut dyn FnMut(Spoken) -> bool) -> Result<()> {
        match self {
            Self::Claude(path) => claude::message::said(path, each),
            Self::Codex(path) => codex::message::said(path, each),
            Self::Gemini(path) => gemini::session::said_all(path, each),
            Self::Opencode { database, session } => opencode::database::said_all(database, session, each),
            Self::Grok(path) => grok::session::said_all(path, each),
            Self::Cursor(path) => cursor::store::said_all(path, each),
        }
    }
}

pub(crate) struct Look<'a> {
    pub(crate) pid: i32,
    pub(crate) args: Vec<String>,
    pub(crate) title: String,
    pub(crate) screen: &'a dyn Fn() -> String,
    pub(crate) claude_dir: Option<&'a Path>,
}

#[derive(Debug)]
enum Reader {
    Claude(claude::Reader),
    Codex(codex::Reader),
    Gemini(gemini::Reader),
    Opencode(opencode::Reader),
    Grok(grok::Reader),
    Cursor(cursor::Reader),
}

impl Reader {
    fn new(kind: Known) -> Self {
        match kind {
            Known::Claude => Self::Claude(claude::Reader::default()),
            Known::Codex => Self::Codex(codex::Reader::default()),
            Known::Gemini => Self::Gemini(gemini::Reader::default()),
            Known::Opencode => Self::Opencode(opencode::Reader::default()),
            Known::Grok => Self::Grok(grok::Reader::default()),
            Known::Cursor => Self::Cursor(cursor::Reader::default()),
        }
    }

    fn update(
        &mut self,
        look: &Look<'_>,
        since: SystemTime,
        shown: &mut Option<Context>,
        ledger: &mut Ledger,
    ) -> Option<SystemTime> {
        match self {
            Self::Claude(reader) => reader.update(look, since, shown, ledger),
            Self::Codex(reader) => reader.update(look, since, shown, ledger),
            Self::Gemini(reader) => reader.update(look, since, shown, ledger),
            Self::Opencode(reader) => reader.update(look, since, shown, ledger),
            Self::Grok(reader) => reader.update(look, since, shown, ledger),
            Self::Cursor(reader) => reader.update(look, since, shown, ledger),
        }
    }

    fn activity(&self, title: &str) -> Option<Activity> {
        match self {
            Self::Claude(reader) => Some(reader.activity(title)),
            Self::Codex(reader) => Some(reader.activity(title)),
            Self::Gemini(_) => gemini::Reader::activity(title),
            Self::Opencode(reader) => reader.activity(),
            Self::Grok(reader) => reader.activity(title),
            Self::Cursor(reader) => Some(reader.activity(title)),
        }
    }

    fn record(&self) -> Option<Record> {
        match self {
            Self::Claude(reader) => reader.record(),
            Self::Codex(reader) => reader.record(),
            Self::Gemini(reader) => reader.record(),
            Self::Opencode(reader) => reader.record(),
            Self::Grok(reader) => reader.record(),
            Self::Cursor(reader) => reader.record(),
        }
    }

    fn conversation(&self) -> Option<&str> {
        match self {
            Self::Claude(reader) => reader.conversation(),
            Self::Codex(reader) => reader.conversation(),
            Self::Gemini(reader) => reader.conversation(),
            Self::Opencode(_) => None,
            Self::Grok(reader) => reader.conversation(),
            Self::Cursor(reader) => reader.conversation(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Pane {
    agent: Option<(i32, SystemTime)>,
    kind: Option<Known>,
    shown: Option<Context>,
    prompted: Option<SystemTime>,
    ledger: Ledger,
    reader: Option<Reader>,
}

impl Pane {
    pub(crate) fn kind(&self) -> Option<Known> {
        self.kind
    }

    pub(crate) fn agent(&self) -> Option<(i32, SystemTime)> {
        self.agent
    }

    pub(crate) fn take_counted(&mut self) -> agents::stats::Counted {
        self.ledger.take()
    }

    pub(crate) fn update(&mut self, kind: Option<Known>, look: &Look<'_>) {
        let Some(kind) = kind else {
            *self = Self::default();
            return;
        };
        if self.agent.is_none_or(|(seen, _)| seen != look.pid) || self.kind != Some(kind) {
            *self = Self { agent: Some((look.pid, SystemTime::now())), kind: Some(kind), ..Self::default() };
        }
        if let Some((_, since)) = self.agent {
            let reader = self.reader.get_or_insert_with(|| Reader::new(kind));
            self.prompted = self.prompted.max(reader.update(look, since, &mut self.shown, &mut self.ledger));
        }
    }

    pub(crate) fn context(&self) -> Option<&Context> {
        self.shown.as_ref()
    }

    pub(crate) fn prompted(&self) -> Option<SystemTime> {
        self.prompted
    }

    pub(crate) fn activity(&self, title: &str) -> Option<Activity> {
        self.reader.as_ref()?.activity(title)
    }

    pub(crate) fn record_for(&self, kind: Known) -> Option<Record> {
        self.reader.as_ref().filter(|_| self.kind == Some(kind))?.record()
    }

    pub(crate) fn conversation(&self) -> Option<&str> {
        self.reader.as_ref()?.conversation()
    }
}

#[cfg(test)]
impl Pane {
    pub(in crate::agents) fn watching(pid: i32, kind: Known, since: SystemTime) -> Self {
        Self { agent: Some((pid, since)), kind: Some(kind), ..Self::default() }
    }
}

pub(in crate::agents) fn percent(tokens: u64, window: u64) -> u16 {
    let window = window.max(1);
    let rounded = tokens.saturating_mul(100).saturating_add(window / 2) / window;
    u16::try_from(rounded.min(100)).unwrap_or(100)
}

#[cfg(test)]
pub(in crate::agents) mod tests;
