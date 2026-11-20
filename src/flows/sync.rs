use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::store::same_name;
use crate::persist;
use crate::time::Stamp;

const KEPT: usize = 256;
pub(crate) const SKEW: Duration = Duration::from_mins(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Change {
    Deleted,
    Hidden,
    Restored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Event {
    pub(crate) name: String,
    pub(crate) change: Change,
    pub(crate) at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Journal {
    pub(crate) events: Vec<Event>,
    pub(crate) synced: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) retry: BTreeMap<String, Vec<Event>>,
}

impl Journal {
    pub(crate) fn read(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub(crate) fn write(&self, path: &Path) -> std::io::Result<()> {
        persist::save(path, self)
    }

    pub(crate) fn record(&mut self, name: &str, change: Change, at: &str) {
        self.forget(name);
        self.events.push(Event { name: name.trim().to_string(), change, at: at.to_string() });
        let over = self.events.len().saturating_sub(KEPT);
        self.events.drain(..over);
    }

    pub(crate) fn forget(&mut self, name: &str) -> bool {
        let before = self.events.len();
        self.events.retain(|event| !same_name(&event.name, name));
        self.events.len() != before
    }

    pub(crate) fn synced(&self, destination: &str) -> Option<SystemTime> {
        self.synced.get(destination).map(String::as_str).and_then(Stamp::parse)
    }

    pub(crate) fn round_ended(&mut self, destination: &str, started: SystemTime, retry: Vec<Event>) {
        self.synced.insert(destination.to_string(), Stamp(started).to_string());
        if retry.is_empty() {
            self.retry.remove(destination);
        } else {
            self.retry.insert(destination.to_string(), retry);
        }
    }
}

pub(crate) fn not_after(changed: Option<SystemTime>, now: SystemTime) -> Option<SystemTime> {
    changed.map(|changed| changed.min(now))
}

pub(crate) fn in_the_future(changed: &str, now: SystemTime) -> bool {
    Stamp::parse(changed).is_some_and(|changed| changed > now + SKEW)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held<S> {
    pub(crate) name: String,
    pub(crate) changed: Option<SystemTime>,
    pub(crate) spec: S,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ours<S> {
    pub(crate) flows: Vec<Held<S>>,
    pub(crate) hidden: Vec<String>,
    pub(crate) events: Vec<Event>,
    pub(crate) synced: Option<SystemTime>,
    pub(crate) retry: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Theirs<S> {
    pub(crate) flows: Vec<Held<S>>,
    pub(crate) hidden: Vec<String>,
    pub(crate) builtins: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    Send(String),
    Delete(String),
    Hide(String),
    Restore(String),
}

impl Op {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Send(name) | Self::Delete(name) | Self::Hide(name) | Self::Restore(name) => name,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) ops: Vec<Op>,
    pub(crate) same: Vec<String>,
    pub(crate) kept: Vec<String>,
}

fn newer(ours: Option<SystemTime>, theirs: Option<SystemTime>) -> bool {
    match (ours, theirs) {
        (Some(ours), Some(theirs)) => ours > theirs,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn find<'a, S>(flows: &'a [Held<S>], name: &str) -> Option<&'a Held<S>> {
    flows.iter().find(|held| same_name(&held.name, name))
}

fn named(names: &[String], name: &str) -> bool {
    names.iter().any(|other| same_name(other, name))
}

pub(crate) fn plan<S: PartialEq>(ours: &Ours<S>, theirs: &Theirs<S>) -> Plan {
    let mut plan = Plan::default();
    for mine in &ours.flows {
        match find(&theirs.flows, &mine.name) {
            Some(there) if newer(there.changed, mine.changed) => plan.kept.push(mine.name.clone()),
            Some(there) if !newer(mine.changed, there.changed) && there.spec == mine.spec => {
                plan.same.push(mine.name.clone());
            }
            _ => plan.ops.push(Op::Send(mine.name.clone())),
        }
    }
    for event in pending(ours) {
        if find(&ours.flows, &event.name).is_some() {
            continue;
        }
        match changed_there(&event, theirs) {
            Some(op) => plan.ops.push(op),
            None if newer_there(&event, theirs) => plan.kept.push(event.name.clone()),
            None => {}
        }
    }
    plan
}

pub(crate) fn pending<S>(ours: &Ours<S>) -> Vec<Event> {
    let mut pending = events(ours);
    for event in &ours.retry {
        let later = pending.iter().any(|newer| same_name(&newer.name, &event.name));
        let current = ours.events.iter().all(|e| !same_name(&e.name, &event.name) || e == event);
        if !later && current {
            pending.push(event.clone());
        }
    }
    pending
}

fn events<S>(ours: &Ours<S>) -> Vec<Event> {
    let Some(synced) = ours.synced else {
        let hidden = |name: &String| {
            let recorded = ours.events.iter().find(|e| e.change == Change::Hidden && same_name(&e.name, name));
            recorded.cloned().unwrap_or_else(|| Event { name: name.clone(), change: Change::Hidden, at: String::new() })
        };
        return ours.hidden.iter().map(hidden).collect();
    };
    ours.events.iter().filter(|event| Stamp::parse(&event.at).is_some_and(|at| at > synced)).cloned().collect()
}

fn newer_there<S>(event: &Event, theirs: &Theirs<S>) -> bool {
    let at = Stamp::parse(&event.at);
    find(&theirs.flows, &event.name).is_some_and(|there| newer(there.changed, at))
}

fn changed_there<S>(event: &Event, theirs: &Theirs<S>) -> Option<Op> {
    if newer_there(event, theirs) {
        return None;
    }
    let file = find(&theirs.flows, &event.name).is_some();
    let builtin = named(&theirs.builtins, &event.name);
    let hidden = named(&theirs.hidden, &event.name);
    let name = event.name.clone();
    match event.change {
        Change::Deleted if file && builtin => Some(Op::Restore(name)),
        Change::Deleted if file => Some(Op::Delete(name)),
        Change::Hidden if builtin && (file || !hidden) => Some(Op::Hide(name)),
        Change::Restored if builtin && (file || hidden) => Some(Op::Restore(name)),
        Change::Deleted | Change::Hidden | Change::Restored => None,
    }
}

#[cfg(test)]
mod tests;
