use std::time::{Instant, SystemTime};

use serde_json::Value;

use super::{Pending, Stage, json};
use crate::agents::Known;
use crate::agents::usage::{self, Key};
use crate::app::App;
use crate::app::usage_dialog::{active, labelled};
use crate::control::usage::PlanInfo;
use crate::control::{Done, Plans, Usage};

impl App {
    pub(super) fn usage_request(&mut self, client: u64, request: &Usage, now: Instant) -> Option<Value> {
        let probes = self.usage_probes();
        let keys: Vec<Key> = probes.iter().map(|(agent, account)| usage::key(*agent, account.as_ref())).collect();
        if request.cached {
            return Some(json(&self.plans(&keys, now)));
        }
        self.start_probes(probes);
        let waiting = keys
            .into_iter()
            .map(|key| {
                let answers = self.usages.state.probe_of(key.0, key.1.as_deref()).answers();
                (key, answers)
            })
            .collect();
        let key = self.wire.requests.key();
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Usage(waiting) };
        self.wire.requests.pending.push(pending);
        None
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
        }
    }
}
