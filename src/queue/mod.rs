pub(crate) mod clock;
pub(crate) mod form;
pub(crate) mod issue;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub(crate) const VERSION: u32 = 1;
pub(crate) const GRACE: i64 = 600;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Place {
    pub(crate) project: PathBuf,
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) workspace_name: Option<String>,
    #[serde(default)]
    pub(crate) workspace_id: Option<u64>,
    #[serde(default)]
    pub(crate) server: String,
    pub(crate) branch: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Task {
    pub(crate) at: i64,
    pub(crate) zone: String,
    pub(crate) label: String,
    pub(crate) agent: String,
    pub(crate) command: String,
    pub(crate) program: String,
    pub(crate) mode: Option<String>,
    pub(crate) prompt: String,
    pub(crate) submit: bool,
    pub(crate) place: Place,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub(crate) enum State {
    Pending,
    Starting,
    Started { pane: u64, server: String },
    Failed { reason: String },
    Missed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: u64,
    #[serde(flatten)]
    pub(crate) task: Task,
    #[serde(flatten)]
    pub(crate) state: State,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    pub(crate) next_id: u64,
    pub(crate) entries: Vec<Entry>,
}

#[derive(Debug, Default)]
pub(crate) struct Queue {
    pub(crate) entries: Vec<Entry>,
    next_id: u64,
}

impl Queue {
    pub(crate) fn add(&mut self, task: Task) -> Result<u64, String> {
        let id = self.next_id.checked_add(1).ok_or("the queue has no ids left")?;
        self.next_id = id;
        self.entries.push(Entry { id, task, state: State::Pending });
        self.sort();
        Ok(id)
    }

    pub(crate) fn edit(&mut self, id: u64, task: Task) -> Result<(), String> {
        let entry = self.entry_mut(id).ok_or_else(|| missing(id))?;
        if matches!(entry.state, State::Starting | State::Started { .. }) {
            return Err("this entry already started; add a new entry instead".into());
        }
        entry.task = task;
        entry.state = State::Pending;
        self.sort();
        Ok(())
    }

    fn sort(&mut self) {
        self.entries.sort_by_key(|entry| (entry.task.at, entry.id));
    }

    pub(crate) fn entry(&self, id: u64) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub(crate) fn entry_mut(&mut self, id: u64) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    pub(crate) fn remove(&mut self, id: u64) -> Result<(), String> {
        let at = self.entries.iter().position(|entry| entry.id == id).ok_or_else(|| missing(id))?;
        self.entries.remove(at);
        Ok(())
    }

    pub(crate) fn clear_started(&mut self) {
        self.entries.retain(|entry| !matches!(entry.state, State::Started { .. }));
    }

    pub(crate) fn pending(&self) -> usize {
        self.entries.iter().filter(|entry| entry.state == State::Pending).count()
    }

    pub(crate) fn next(&self, now: i64) -> Option<Duration> {
        self.entries
            .iter()
            .filter(|entry| entry.state == State::Pending)
            .map(|entry| Duration::from_secs(u64::try_from(entry.task.at.saturating_sub(now)).unwrap_or_default()))
            .min()
    }

    pub(crate) fn due(&mut self, now: i64) -> Vec<u64> {
        let mut due = Vec::new();
        for entry in self.entries.iter_mut().filter(|entry| entry.state == State::Pending && entry.task.at <= now) {
            if now.saturating_sub(entry.task.at) > GRACE {
                entry.state = State::Missed;
            } else {
                due.push(entry.id);
            }
        }
        due
    }

    pub(crate) fn saved(&self) -> Saved {
        Saved { version: VERSION, next_id: self.next_id, entries: self.entries.clone() }
    }
}

pub(crate) fn missing(id: u64) -> String {
    format!("there is no queue entry {id}; `cornercase queue list` lists them")
}

pub(crate) fn read_saved(path: &Path) -> Option<Saved> {
    let saved = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok());
    let saved = saved.filter(|saved| saved.version == VERSION)?;
    let mut ids = HashSet::new();
    if saved.entries.iter().any(|entry| entry.id == 0 || !ids.insert(entry.id)) {
        return None;
    }
    Some(saved)
}

pub(crate) fn load(path: &Path, now: i64) -> Queue {
    let Some(saved) = read_saved(path) else { return Queue::default() };
    let mut queue = Queue { entries: saved.entries, next_id: saved.next_id };
    queue.next_id = queue.entries.iter().map(|entry| entry.id).max().unwrap_or_default().max(queue.next_id);
    queue.sort();
    for entry in &mut queue.entries {
        if entry.state == State::Starting {
            entry.state = State::Failed {
                reason: "the server stopped while this entry was starting; check its workspace before trying again"
                    .into(),
            };
        }
    }
    queue.due(now);
    queue
}

#[cfg(test)]
mod tests;
