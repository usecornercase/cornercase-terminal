use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::Named;
use crate::agents::context::Record;
use crate::time::Stamp;

pub(crate) mod files;

const VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Event {
    Seen,
    Prompt,
    Status,
    Closed,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkspacePlace {
    pub(crate) label: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: String,
    pub(crate) worktree: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PullPlace {
    pub(crate) number: u64,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) ci: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) v: u8,
    pub(crate) at: String,
    pub(crate) event: Event,
    pub(crate) to: Option<String>,
    pub(crate) agent: String,
    pub(crate) account: Option<String>,
    pub(crate) conversation: Option<String>,
    pub(crate) record: Option<Pointer>,
    pub(crate) pane: Option<u64>,
    pub(crate) boot: Option<String>,
    pub(crate) project: Option<Named>,
    pub(crate) workspace: Option<WorkspacePlace>,
    pub(crate) tab: Option<String>,
    pub(crate) pull_request: Option<PullPlace>,
}

impl Entry {
    pub(crate) fn blank(agent: &str) -> Self {
        Self {
            v: VERSION,
            at: String::new(),
            event: Event::Seen,
            to: None,
            agent: agent.to_string(),
            account: None,
            conversation: None,
            record: None,
            pane: None,
            boot: None,
            project: None,
            workspace: None,
            tab: None,
            pull_request: None,
        }
    }

    pub(crate) fn key(&self) -> Option<String> {
        key_of(&self.agent, self.conversation.as_deref(), self.record.as_ref().map(Pointer::path))
    }

    pub(crate) fn seconds(&self) -> Option<i64> {
        let time = Stamp::parse(&self.at)?;
        let since = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
        i64::try_from(since.as_secs()).ok()
    }

    fn at(&self, event: Event, to: Option<&str>, time: SystemTime) -> Self {
        Self { at: Stamp(time).to_string(), event, to: to.map(str::to_string), ..self.clone() }
    }
}

pub(crate) fn key_of(agent: &str, conversation: Option<&str>, record: Option<String>) -> Option<String> {
    let id = conversation.map(str::to_string).or(record)?;
    Some(format!("{agent}:{id}"))
}

#[derive(Debug)]
struct Seen {
    key: String,
    status: Option<String>,
    template: Entry,
}

pub(crate) struct Observed {
    pub(crate) pane: u64,
    pub(crate) key: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) prompts: Vec<SystemTime>,
}

#[derive(Debug, Default)]
pub(crate) struct Journal {
    seen: HashMap<u64, Seen>,
}

impl Journal {
    pub(crate) fn observe(
        &mut self,
        now: SystemTime,
        panes: &[Observed],
        template: &mut dyn FnMut(u64) -> Option<Entry>,
    ) -> Vec<Entry> {
        let mut entries = Vec::new();
        for observed in panes {
            self.pane(now, observed, template, &mut entries);
        }
        let gone: Vec<u64> =
            self.seen.keys().copied().filter(|pane| panes.iter().all(|o| o.pane != *pane || o.key.is_none())).collect();
        for pane in gone {
            if let Some(seen) = self.seen.remove(&pane) {
                entries.push(seen.template.at(Event::Closed, None, now));
            }
        }
        entries
    }

    fn pane(
        &mut self,
        now: SystemTime,
        observed: &Observed,
        template: &mut dyn FnMut(u64) -> Option<Entry>,
        entries: &mut Vec<Entry>,
    ) {
        let Some(key) = observed.key.as_ref() else { return };
        let changed = self.seen.get(&observed.pane).is_none_or(|seen| seen.key != *key);
        if changed {
            if let Some(seen) = self.seen.remove(&observed.pane) {
                entries.push(seen.template.at(Event::Closed, None, now));
            }
            let Some(entry) = template(observed.pane) else { return };
            entries.push(entry.at(Event::Seen, None, now));
            self.seen.insert(observed.pane, Seen { key: key.clone(), status: None, template: entry });
        }
        let Some(seen) = self.seen.get_mut(&observed.pane) else { return };
        for prompt in &observed.prompts {
            entries.push(seen.template.at(Event::Prompt, None, *prompt));
        }
        if observed.status.is_some() && observed.status != seen.status {
            entries.push(seen.template.at(Event::Status, observed.status.as_deref(), now));
            seen.status.clone_from(&observed.status);
        }
    }
}

#[cfg(test)]
mod tests;
