use std::time::{Instant, SystemTime};

use serde_json::Value;

use super::machines::unknown;
use super::{Handled, Pending, Reply, Stage, json};
use crate::agents::stats::View;
use crate::agents::usage::Key;
use crate::app::App;
use crate::app::machine_usage::Asked;
use crate::control::contract::Refusal;
use crate::control::usage::MachineStats;
use crate::control::{Done, Plans, Stats};
use crate::machines::EVERY;

#[derive(Clone, Copy)]
pub(super) enum Of {
    Stats,
    Usage,
}

pub(super) struct Gathering {
    pub(super) of: Of,
    pub(super) one: Option<u64>,
    pub(super) links: Vec<u64>,
    pub(super) local: Vec<(Key, u64)>,
}

pub(super) fn every(machine: &str) -> bool {
    machine.eq_ignore_ascii_case(EVERY)
}

impl App {
    pub(super) fn stats_request(&mut self, client: u64, request: &Stats, now: Instant) -> Handled {
        match request.machine.as_deref() {
            None => Ok(Some(json(&self.stats_view(SystemTime::now())))),
            Some(machine) if every(machine) => {
                let links = self.ask_every_machine::<View>();
                self.gather(client, Gathering { of: Of::Stats, one: None, links, local: Vec::new() }, now)
            }
            Some(name) => self.ask_one_machine::<View>(client, name, Of::Stats, now),
        }
    }

    pub(super) fn machine_link(&self, name: &str) -> Result<u64, Refusal> {
        let held = self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(name));
        held.map(|held| held.link.id()).ok_or_else(|| unknown(name))
    }

    pub(super) fn ask_one_machine<T: Asked>(&mut self, client: u64, name: &str, of: Of, now: Instant) -> Handled {
        let link = self.machine_link(name)?;
        if self.ask_machine::<T>(name).is_none() {
            return self.machine_answer::<T>(link).map(Some);
        }
        self.gather(client, Gathering { of, one: Some(link), links: vec![link], local: Vec::new() }, now)
    }

    pub(super) fn gather(&mut self, client: u64, gathering: Gathering, now: Instant) -> Handled {
        if let Some(reply) = self.gathered(&gathering, now) {
            return reply.map(Some).map_err(Refusal::from);
        }
        let key = self.clients.requests.key();
        let (done, stage) = (Done::default(), Stage::Machines(gathering));
        self.clients.requests.pending.push(Pending { client: Some(client), key, timeout: None, done, stage });
        Ok(None)
    }

    pub(super) fn gathered(&self, gathering: &Gathering, now: Instant) -> Option<Reply> {
        let Gathering { of, one, links, local } = gathering;
        let answered = match of {
            Of::Stats => self.machines_answered::<View>(links),
            Of::Usage => self.machines_answered::<Plans>(links),
        };
        if !answered {
            return None;
        }
        let reply = match (of, one) {
            (Of::Stats, Some(link)) => self.machine_answer::<View>(*link),
            (Of::Usage, Some(link)) => self.machine_answer::<Plans>(*link),
            (Of::Stats, None) => Ok(self.every_stats(SystemTime::now())),
            (Of::Usage, None) => Ok(json(&self.every_usage(self.usage_read(local, now)?))),
        };
        Some(reply.map_err(String::from))
    }

    fn every_stats(&self, now: SystemTime) -> Value {
        let (view, machines) = self.summed_stats(now);
        json(&MachineStats { view, machine: None, machines })
    }
}
