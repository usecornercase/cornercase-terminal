use std::time::{Instant, SystemTime};

use serde_json::Value;

use super::{Pending, Stage, json};
use crate::agents::Known;
use crate::app::App;
use crate::control::usage::PlanInfo;
use crate::control::{Done, Plans, Usage};

impl App {
    pub(super) fn usage_request(&mut self, client: u64, request: &Usage, now: Instant) -> Option<Value> {
        let agents = self.usage_agents();
        if request.cached {
            return Some(json(&self.plans(&agents, now)));
        }
        self.start_probes(agents.clone());
        let waiting = agents.into_iter().map(|agent| (agent, self.usage.probe(agent).answers())).collect();
        let key = self.requests.key();
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Usage(waiting) };
        self.requests.pending.push(pending);
        None
    }

    pub(super) fn usage_read(&self, waiting: &[(Known, u64)], now: Instant) -> Option<Plans> {
        waiting.iter().all(|&(agent, before)| self.usage.probe(agent).answers() > before).then(|| {
            let agents: Vec<Known> = waiting.iter().map(|&(agent, _)| agent).collect();
            self.plans(&agents, now)
        })
    }

    fn plans(&self, agents: &[Known], now: Instant) -> Plans {
        let clock = SystemTime::now();
        Plans { agents: agents.iter().map(|&agent| PlanInfo::of(agent, self.usage.probe(agent), now, clock)).collect() }
    }
}
