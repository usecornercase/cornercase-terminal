use std::collections::BTreeMap;
use std::time::SystemTime;

use super::App;
use crate::agents::Known;
use crate::agents::context::{Record, Said, Stamp};
use crate::app::event::AppEvent;
use crate::control::{Event, Ids, What};
use crate::error;
use crate::log::{self, Job, Level};
use crate::project::Hierarchy as _;

const RETRIES: u8 = 3;

#[derive(Default)]
pub(super) struct LastMessages {
    panes: BTreeMap<u64, Watched>,
    primed: bool,
    reads: u64,
}

pub(super) struct Due {
    pane: u64,
    agent: Known,
    record: Record,
    read: u64,
}

struct Watched {
    seen: Option<(Record, Stamp)>,
    kept: Kept,
    reading: Option<u64>,
    failed: u8,
}

enum Kept {
    Nothing,
    Read(Option<Said>),
}

impl LastMessages {
    pub(super) fn unprime(&mut self) {
        self.primed = false;
    }

    pub(super) fn clear(&mut self) {
        *self = Self { reads: self.reads, ..Self::default() };
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    #[cfg(all(test, unix))]
    pub(super) fn settled(&self, pane: u64, stamp: &Stamp) -> bool {
        self.panes.get(&pane).is_some_and(|watched| {
            watched.reading.is_none() && watched.seen.as_ref().is_some_and(|(_, seen)| seen == stamp)
        })
    }

    fn step(&mut self, candidates: Vec<(u64, Known, Record)>) -> Vec<Due> {
        self.panes.retain(|pane, _| candidates.iter().any(|(candidate, ..)| candidate == pane));
        let due = candidates
            .into_iter()
            .filter_map(|(pane, agent, record)| self.due(pane, &record).map(|read| Due { pane, agent, record, read }))
            .collect();
        self.primed = true;
        due
    }

    fn told(&mut self, pane: u64, read: u64, said: error::Result<Option<Said>>) -> bool {
        let Some(watched) = self.panes.get_mut(&pane).filter(|watched| watched.reading == Some(read)) else {
            return false;
        };
        watched.reading = None;
        let Ok(said) = said else {
            watched.retry();
            return false;
        };
        watched.failed = 0;
        let changed = said.is_some() && matches!(&watched.kept, Kept::Read(kept) if *kept != said);
        watched.kept = Kept::Read(said);
        changed
    }

    fn due(&mut self, pane: u64, record: &Record) -> Option<u64> {
        let (primed, next) = (self.primed, self.reads + 1);
        let watched = self.panes.entry(pane).or_insert_with(|| Watched {
            seen: None,
            kept: if primed { Kept::Read(None) } else { Kept::Nothing },
            reading: None,
            failed: 0,
        });
        if watched.reading.is_some() {
            return None;
        }
        let stamp = record.stamp();
        match &watched.seen {
            Some((seen, stamped)) if seen == record && *stamped == stamp => return None,
            Some(_) => watched.failed = 0,
            None => {}
        }
        watched.seen = Some((record.clone(), stamp));
        watched.reading = Some(next);
        self.reads = next;
        Some(next)
    }
}

impl Watched {
    fn retry(&mut self) {
        if self.failed < RETRIES {
            self.failed += 1;
            self.seen = None;
        }
    }
}

impl App {
    pub(super) fn watch_last_messages(&mut self) {
        if !self.clients.events.listening() {
            self.clients.events.last_messages.clear();
            return;
        }
        let candidates = self.last_message_candidates();
        let due = self.clients.events.last_messages.step(candidates);
        for due in due {
            self.read_last_message(due);
        }
    }

    fn last_message_candidates(&self) -> Vec<(u64, Known, Record)> {
        let events = &self.clients.events;
        self.projects
            .terms()
            .filter(|term| events.wants_pane(term.id))
            .filter_map(|term| {
                let kind = term.context.kind()?;
                Some((term.id, kind, term.context.record_for(kind)?))
            })
            .collect()
    }

    fn read_last_message(&self, Due { pane, agent, record, read }: Due) {
        let job =
            Job::new(Level::Debug, "control", "last message changed").with("pane", pane).with("agent", agent.name());
        self.spawn_job(
            job,
            move || record.last_message(),
            move |said, _| Some(AppEvent::LastMessageSeen { pane, agent, read, said }),
        );
    }

    pub(in crate::app) fn last_message_seen(
        &mut self,
        (pane, agent): (u64, Known),
        read: u64,
        said: error::Result<Option<Said>>,
    ) {
        if !self.clients.events.last_messages.told(pane, read, said) {
            return;
        }
        let Some(at) = self.projects.tab_of_pane(pane) else { return };
        let event = Event {
            time: crate::time::Stamp(SystemTime::now()).to_string(),
            what: What::LastMessage { agent: agent.name().to_string() },
            ids: Ids { pane: Some(pane), ..self.ids_of_tab(at) },
        };
        log::debug!("control", "last message", pane = pane, agent = agent.name());
        self.clients.events.publish(&[event]);
    }
}

#[cfg(test)]
mod tests;
