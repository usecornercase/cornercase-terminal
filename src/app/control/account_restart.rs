use std::collections::BTreeSet;

use super::accounts::provider;
use super::{Handled, Pending, Stage, json, none};
use crate::agents::Known;
use crate::app::App;
use crate::app::account_restart::Restart;
use crate::control::contract::Refusal;
use crate::control::{AccountRestart, Done, RestartInfo, SkippedPane};
use crate::project::Hierarchy;
use crate::term::Term;

#[derive(Debug, Default)]
pub(super) struct Batch {
    info: RestartInfo,
    left: BTreeSet<u64>,
    asking: bool,
}

impl App {
    pub(in crate::app) fn account_restart_request(&mut self, client: u64, request: &AccountRestart) -> Handled {
        let kind = provider(&request.provider)?;
        self.relist_accounts();
        let to = match &request.account {
            Some(account) => {
                self.known_account(kind, account)?;
                account.clone()
            }
            None => self.chosen(kind).to_string(),
        };
        let mut info = RestartInfo { provider: kind.name().to_string(), account: to.clone(), ..RestartInfo::default() };
        let panes = self.panes_to_restart(request, kind, &mut info)?;
        let key = self.clients.requests.key();
        self.clients.requests.restarting.insert(key, Batch { info, left: BTreeSet::new(), asking: true });
        for pane in panes {
            self.ask_in_batch(key, pane, kind, &to);
        }
        let batch = self.restart_batch(key);
        batch.asking = false;
        if batch.left.is_empty() {
            let batch = self.clients.requests.restarting.remove(&key).unwrap_or_default();
            return Ok(Some(json(&answer(batch.info))));
        }
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Reading };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    fn panes_to_restart(
        &self,
        request: &AccountRestart,
        kind: Known,
        info: &mut RestartInfo,
    ) -> Result<Vec<u64>, Refusal> {
        if request.panes.is_empty() {
            let (stale, skipped) = self.panes_on_other_accounts(kind, &info.account);
            info.skipped = skipped.into_iter().map(|(pane, why)| SkippedPane { pane, why }).collect();
            return Ok(stale);
        }
        let mut panes: Vec<u64> = Vec::new();
        for pane in &request.panes {
            self.projects.tab_of_pane(*pane).ok_or_else(|| none("pane", *pane))?;
            if !panes.contains(pane) {
                panes.push(*pane);
            }
        }
        Ok(panes)
    }

    fn ask_in_batch(&mut self, key: u64, pane: u64, kind: Known, to: &str) {
        let runs = self.projects.pane_by(pane).and_then(Term::agent_account).is_some_and(|(k, _)| k == kind);
        if !runs {
            self.respawn_in_batch(key, pane, kind, to);
            return;
        }
        self.restart_batch(key).left.insert(pane);
        let asked = self.ask_restart_for(pane, kind, to, Some(key));
        let batch = self.restart_batch(key);
        match asked {
            Ok(Restart::Now) => {}
            Ok(Restart::Waiting) => {
                batch.left.remove(&pane);
                batch.info.waiting.push(pane);
            }
            Err(why) => {
                batch.left.remove(&pane);
                batch.info.skipped.push(SkippedPane { pane, why });
            }
        }
    }

    fn respawn_in_batch(&mut self, key: u64, pane: u64, kind: Known, to: &str) {
        let respawned = self.respawn_shell(pane, kind, to);
        let batch = self.restart_batch(key);
        match respawned {
            Ok(()) => batch.info.restarted.push(pane),
            Err(why) => batch.info.skipped.push(SkippedPane { pane, why }),
        }
    }

    pub(in crate::app) fn account_restarted(&mut self, key: u64, pane: u64, outcome: Result<(), String>) {
        let Some(batch) = self.clients.requests.restarting.get_mut(&key) else { return };
        batch.left.remove(&pane);
        match outcome {
            Ok(()) => batch.info.restarted.push(pane),
            Err(why) => batch.info.skipped.push(SkippedPane { pane, why }),
        }
        if batch.asking || !batch.left.is_empty() {
            return;
        }
        let info = self.clients.requests.restarting.remove(&key).unwrap_or_default().info;
        let Some(pending) = self.clients.requests.take(key) else { return };
        self.refuse_or_answer(pending.client, Ok(json(&answer(info))));
    }

    fn restart_batch(&mut self, key: u64) -> &mut Batch {
        self.clients.requests.restarting.entry(key).or_default()
    }
}

fn answer(mut info: RestartInfo) -> RestartInfo {
    info.restarted.sort_unstable();
    info.waiting.sort_unstable();
    info.skipped.sort_by_key(|skipped| skipped.pane);
    info
}
