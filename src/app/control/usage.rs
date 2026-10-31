use std::time::{Instant, SystemTime};

use serde_json::Value;

use super::stats::{Gathering, Of, every};
use super::{Handled, Pending, Stage, json};
use crate::accounts::Account;
use crate::agents::Known;
use crate::agents::usage::{self, Key};
use crate::app::App;
use crate::app::usage_dialog::{active, labelled};
use crate::control::usage::{MachineUsage, PlanInfo};
use crate::control::{Done, Plans, Usage};

impl App {
    pub(super) fn usage_request(&mut self, client: u64, request: &Usage, now: Instant) -> Handled {
        match request.machine.as_deref() {
            None => Ok(self.usage_here(client, request.cached, now)),
            Some(machine) if every(machine) => self.usage_everywhere(client, request.cached, now),
            Some(name) if request.cached => self.machine_answer::<Plans>(self.machine_link(name)?).map(Some),
            Some(name) => self.ask_one_machine::<Plans>(client, name, Of::Usage, now),
        }
    }

    fn usage_here(&mut self, client: u64, cached: bool, now: Instant) -> Option<Value> {
        if cached {
            return Some(json(&self.plans(&self.usage_keys(), now)));
        }
        let waiting = self.probe_here();
        let key = self.clients.requests.key();
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Usage(waiting) };
        self.clients.requests.pending.push(pending);
        None
    }

    fn usage_everywhere(&mut self, client: u64, cached: bool, now: Instant) -> Handled {
        if cached {
            return Ok(Some(json(&self.every_usage(self.plans(&self.usage_keys(), now)))));
        }
        let local = self.probe_here();
        let links = self.ask_every_machine::<Plans>();
        self.gather(client, Gathering { of: Of::Usage, one: None, links, local }, now)
    }

    fn usage_keys(&self) -> Vec<Key> {
        keys(&self.usage_probes())
    }

    fn probe_here(&mut self) -> Vec<(Key, u64)> {
        let probes = self.usage_probes();
        let keys = keys(&probes);
        self.start_probes(probes);
        keys.into_iter()
            .map(|key| {
                let answers = self.usages.state.probe_of(key.0, key.1.as_deref()).answers();
                (key, answers)
            })
            .collect()
    }

    pub(super) fn usage_read(&self, waiting: &[(Key, u64)], now: Instant) -> Option<Plans> {
        let answered = |((agent, account), before): &(Key, u64)| {
            self.usages.state.probe_of(*agent, account.as_deref()).answers() > *before
        };
        waiting.iter().all(answered).then(|| {
            let keys: Vec<Key> = waiting.iter().map(|(key, _)| key.clone()).collect();
            self.plans(&keys, now)
        })
    }

    pub(super) fn every_usage(&self, here: Plans) -> Plans {
        let machines = self.machine_reads::<Plans>().into_iter();
        let machines = machines.map(|(machine, plans)| MachineUsage {
            machine,
            agents: plans.map(|plans| plans.agents).unwrap_or_default(),
        });
        Plans { machines: machines.collect(), ..here }
    }

    fn plans(&self, keys: &[Key], now: Instant) -> Plans {
        let clock = SystemTime::now();
        let listed: Vec<(Known, Option<&str>)> =
            keys.iter().map(|(agent, account)| (*agent, account.as_deref())).collect();
        let chosen = self.config.selected_accounts();
        Plans {
            agents: listed
                .iter()
                .map(|&(agent, account)| {
                    let probe = self.usages.state.probe_of(agent, account);
                    let key = (agent, account);
                    let label = labelled(&self.config, &listed, key);
                    PlanInfo::of(agent, probe, now, clock).for_account(account, label, active(&listed, chosen, key))
                })
                .collect(),
            ..Plans::default()
        }
    }
}

fn keys(probes: &[(Known, Option<Account>)]) -> Vec<Key> {
    probes.iter().map(|(agent, account)| usage::key(*agent, account.as_ref())).collect()
}
