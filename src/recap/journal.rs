use std::collections::HashMap;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::Named;
use crate::agents::context::Pointer;
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
    let id = record.or_else(|| conversation.map(str::to_string))?;
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
    pub(crate) conversation: Option<String>,
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
        if observed.conversation.is_some() {
            seen.template.conversation.clone_from(&observed.conversation);
        }
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
