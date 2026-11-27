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

#[derive(Default)]
pub(super) struct LastMessages {
    panes: BTreeMap<u64, Watched>,
    primed: bool,
}

struct Watched {
    seen: Option<(Record, Stamp)>,
    kept: Kept,
    reading: bool,
}

enum Kept {
    Nothing,
    Read(Option<Said>),
}

impl LastMessages {
    pub(super) fn unprime(&mut self) {
        self.primed = false;
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.panes.is_empty()
    }

    #[cfg(all(test, unix))]
    pub(super) fn settled(&self, pane: u64, stamp: &Stamp) -> bool {
        self.panes
            .get(&pane)
            .is_some_and(|watched| !watched.reading && watched.seen.as_ref().is_some_and(|(_, seen)| seen == stamp))
    }

    fn step(&mut self, candidates: Vec<(u64, Known, Record)>) -> Vec<(u64, Known, Record)> {
        self.panes.retain(|pane, _| candidates.iter().any(|(candidate, ..)| candidate == pane));
        let due = candidates.into_iter().filter(|(pane, _, record)| self.due(*pane, record)).collect();
        self.primed = true;
        due
    }

    fn told(&mut self, pane: u64, said: error::Result<Option<Said>>) -> bool {
        let Some(watched) = self.panes.get_mut(&pane) else { return false };
        watched.reading = false;
        let Ok(said) = said else { return false };
        let changed = said.is_some() && matches!(&watched.kept, Kept::Read(kept) if *kept != said);
        watched.kept = Kept::Read(said);
        changed
    }

    fn due(&mut self, pane: u64, record: &Record) -> bool {
        let primed = self.primed;
        let watched = self.panes.entry(pane).or_insert_with(|| Watched {
            seen: None,
            kept: if primed { Kept::Read(None) } else { Kept::Nothing },
            reading: false,
        });
        if watched.reading {
            return false;
        }
        let stamp = record.stamp();
        if watched.seen.as_ref().is_some_and(|(seen, stamped)| seen == record && *stamped == stamp) {
            return false;
        }
        watched.seen = Some((record.clone(), stamp));
        watched.reading = true;
        true
    }
}

impl App {
    pub(super) fn watch_last_messages(&mut self) {
        if !self.clients.events.listening() {
            self.clients.events.last_messages = LastMessages::default();
            return;
        }
        let candidates = self.last_message_candidates();
        let due = self.clients.events.last_messages.step(candidates);
        for (pane, kind, record) in due {
            self.read_last_message(pane, kind, record);
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

    fn read_last_message(&self, pane: u64, agent: Known, record: Record) {
        let job =
            Job::new(Level::Debug, "control", "last message changed").with("pane", pane).with("agent", agent.name());
        self.spawn_job(
            job,
            move || record.last_message(),
            move |said, _| Some(AppEvent::LastMessageSeen { pane, agent, said }),
        );
    }

    pub(in crate::app) fn last_message_seen(&mut self, pane: u64, agent: Known, said: error::Result<Option<Said>>) {
        if !self.clients.events.last_messages.told(pane, said) {
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
