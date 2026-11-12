pub(crate) mod clock;
pub(crate) mod conversation;
pub(crate) mod form;
pub(crate) mod outcome;
pub(crate) mod repeat;
pub(crate) mod routine;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use repeat::Repeat;

pub(crate) const VERSION: u32 = 2;
pub(crate) const RUNS_KEPT: usize = 10;
pub(crate) const GRACE: i64 = 600;
pub(crate) const UNDOS: usize = 16;
const NAME_MAX: usize = 40;

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) accounts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) repeat: Option<Repeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) flow: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) cleanup: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) paused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) routine: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fresh: Option<bool>,
}

#[expect(clippy::trivially_copy_pass_by_ref, reason = "serde's skip_serializing_if passes a reference")]
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub(crate) enum State {
    Pending,
    Starting,
    Started { pane: u64, server: String },
    Failed { reason: String },
    Missed,
    Removed,
    Kept { pane: u64, server: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: u64,
    #[serde(flatten)]
    pub(crate) task: Task,
    #[serde(flatten)]
    pub(crate) state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) started: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<outcome::Outcome>,
}

impl Entry {
    pub(crate) fn new(id: u64, task: Task, state: State) -> Self {
        Self { id, task, state, started: None, result: None }
    }

    pub(crate) fn is_routine(&self) -> bool {
        self.task.repeat.is_some()
    }

    pub(crate) fn is_run(&self) -> bool {
        self.task.routine.is_some()
    }

    fn clearable(&self) -> bool {
        if self.is_routine() {
            return false;
        }
        match self.state {
            State::Started { .. } => true,
            State::Removed | State::Kept { .. } | State::Failed { .. } | State::Missed => self.is_run(),
            State::Pending | State::Starting => false,
        }
    }

    fn uses_routines(&self) -> bool {
        let task = &self.task;
        task.repeat.is_some()
            || task.routine.is_some()
            || task.flow.is_some()
            || matches!(self.state, State::Removed | State::Kept { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Listed {
    #[serde(flatten)]
    pub(crate) entry: Entry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) next: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) result_at: Option<i64>,
}

impl From<&Entry> for Listed {
    fn from(entry: &Entry) -> Self {
        let next = (entry.is_routine() && !entry.task.paused).then_some(entry.task.at);
        let result_at = entry.result.as_ref().map(|result| result.at);
        let listed = Entry { started: entry.started, ..Entry::new(entry.id, entry.task.clone(), entry.state.clone()) };
        Self { entry: listed, next, result_at }
    }
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
        self.entries.push(Entry::new(id, task, State::Pending));
        self.sort();
        Ok(id)
    }

    pub(crate) fn edit(&mut self, id: u64, task: Task) -> Result<(), String> {
        let entry = self.entry_mut(id).ok_or_else(|| missing(id))?;
        if let Some(routine) = entry.task.routine {
            return Err(format!("this is a run of routine {routine}; edit the routine instead"));
        }
        if entry.is_routine() != task.repeat.is_some() {
            return Err("a one-off entry cannot become a routine, or the other way round; add a new entry".into());
        }
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

    pub(crate) fn remove(&mut self, id: u64) -> Result<Vec<Entry>, String> {
        let entry = self.entry(id).ok_or_else(|| missing(id))?;
        if !entry.is_routine() {
            let at = self.entries.iter().position(|entry| entry.id == id).ok_or_else(|| missing(id))?;
            return Ok(vec![self.entries.remove(at)]);
        }
        if self.runs_of(id).any(|run| run.state == State::Starting) {
            return Err(format!("a run of routine {id} is starting; remove it when it has started"));
        }
        let (gone, kept) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|entry| entry.id == id || entry.task.routine == Some(id));
        self.entries = kept;
        Ok(gone)
    }

    pub(crate) fn clear_started(&mut self) -> Vec<Entry> {
        let (started, kept) = std::mem::take(&mut self.entries).into_iter().partition(Entry::clearable);
        self.entries = kept;
        started
    }

    pub(crate) fn restore(&mut self, entries: Vec<Entry>) {
        for entry in entries {
            if self.entry(entry.id).is_none() {
                self.entries.push(entry);
            }
        }
        self.sort();
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> usize {
        self.entries.iter().filter(|entry| entry.state == State::Pending).count()
    }

    pub(crate) fn next(&self, now: i64) -> Option<Duration> {
        self.entries
            .iter()
            .filter(|entry| entry.state == State::Pending && !entry.task.paused)
            .map(|entry| Duration::from_secs(u64::try_from(entry.task.at.saturating_sub(now)).unwrap_or_default()))
            .min()
    }

    pub(crate) fn due(&mut self, now: i64) -> Vec<u64> {
        let mut due = Vec::new();
        for entry in self
            .entries
            .iter_mut()
            .filter(|entry| entry.state == State::Pending && !entry.is_routine() && entry.task.at <= now)
        {
            if now.saturating_sub(entry.task.at) > GRACE {
                entry.state = State::Missed;
            } else {
                due.push(entry.id);
            }
        }
        due
    }

    pub(crate) fn saved(&self) -> Saved {
        let version = if self.entries.iter().any(Entry::uses_routines) { VERSION } else { 1 };
        Saved { version, next_id: self.next_id, entries: self.entries.clone() }
    }
}

pub(crate) fn missing(id: u64) -> String {
    format!("there is no queue entry {id}; `cornercase queue list` lists them")
}

pub(crate) fn read_saved(path: &Path) -> Option<Saved> {
    let saved = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok());
    let saved = saved.filter(|saved| (1..=VERSION).contains(&saved.version))?;
    let mut ids = HashSet::new();
    if saved.entries.iter().any(|entry| entry.id == 0 || !ids.insert(entry.id)) {
        return None;
    }
    Some(saved)
}

pub(crate) fn prompt_name(prompt: &str) -> String {
    let line = prompt
        .lines()
        .map(|line| crate::text::one_line(&line.replace(char::is_control, " ")))
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    crate::text::first_words(&line, NAME_MAX)
}

fn older_prompt_name(prompt: &str) -> String {
    crate::text::one_line(&prompt.lines().next().unwrap_or_default().replace(char::is_control, ""))
}

fn rename_older(task: &mut Task) {
    if task.label == older_prompt_name(&task.prompt) {
        task.label = prompt_name(&task.prompt);
    }
}

pub(crate) fn load(path: &Path, now: i64) -> Queue {
    let Some(saved) = read_saved(path) else { return Queue::default() };
    let mut queue = Queue { entries: saved.entries, next_id: saved.next_id };
    queue.next_id = queue.entries.iter().map(|entry| entry.id).max().unwrap_or_default().max(queue.next_id);
    queue.sort();
    for entry in &mut queue.entries {
        rename_older(&mut entry.task);
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
