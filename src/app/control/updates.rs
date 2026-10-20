use std::time::Instant;

use super::{App, Done, Handled, Pending, Stage, json};
use crate::control::contract::{Code, Refusal};

impl App {
    pub(super) fn update_check_request(&mut self, client: u64, now: Instant) {
        let key = self.waiting(client);
        self.update.checks.push(key);
        self.start_update_check(now);
    }

    pub(super) fn update_install_request(&mut self, client: u64) -> Handled {
        if self.update.updates.installed {
            return Ok(Some(json(&self.update_info())));
        }
        self.install_update()?;
        let key = self.waiting(client);
        self.update.installs.push(key);
        Ok(None)
    }

    fn waiting(&mut self, client: u64) -> u64 {
        let key = self.clients.requests.key();
        self.clients.requests.pending.push(Pending {
            client: Some(client),
            key,
            timeout: None,
            done: Done::default(),
            stage: Stage::Reading,
        });
        key
    }

    pub(in crate::app) fn answer_update_checks(&mut self, outcome: &Result<(), String>) {
        let keys = std::mem::take(&mut self.update.checks);
        let reply = match outcome {
            Ok(()) => Ok(json(&self.update_info())),
            Err(e) => Err(Refusal::new(Code::Failed, format!("could not check for updates: {e}"))),
        };
        self.answer_waiting(&keys, &reply);
    }

    pub(in crate::app) fn answer_update_installs(&mut self, outcome: &Result<(), String>) {
        let keys = std::mem::take(&mut self.update.installs);
        let reply = match outcome {
            Ok(()) => Ok(json(&self.update_info())),
            Err(e) => {
                let version = self.update.updates.available.as_ref().map_or("", |release| release.version.as_str());
                Err(Refusal::new(Code::Failed, format!("could not install cornercase {version}: {e}")))
            }
        };
        self.answer_waiting(&keys, &reply);
    }

    fn answer_waiting(&mut self, keys: &[u64], reply: &Result<serde_json::Value, Refusal>) {
        for key in keys {
            if let Some(pending) = self.clients.requests.take(*key) {
                self.refuse_or_answer(pending.client, reply.clone());
            }
        }
    }
}
