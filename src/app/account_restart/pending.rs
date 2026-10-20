use std::collections::BTreeMap;

use super::{Moving, Waiting};
use crate::agents::Known;
use crate::log;

#[derive(Default)]
pub(in crate::app) struct Restarts {
    waiting: BTreeMap<u64, Waiting>,
    moving: BTreeMap<u64, Moving>,
    moves: u64,
}

impl Restarts {
    pub(in crate::app) fn start_move(&mut self, pane: u64, notice: String, to: &str) -> u64 {
        self.moves += 1;
        let job = self.moves;
        self.moving.insert(pane, Moving { notice, to: to.to_string(), job, requests: Vec::new() });
        job
    }

    pub(in crate::app) fn moving_to(&self, pane: u64) -> Option<&str> {
        self.moving.get(&pane).map(|moving| moving.to.as_str())
    }

    pub(in crate::app) fn join_move(&mut self, pane: u64, request: Option<u64>) {
        if let Some(moving) = self.moving.get_mut(&pane) {
            moving.requests.extend(request);
        }
    }

    pub(in crate::app) fn moving_notice(&self, pane: u64) -> Option<&str> {
        self.moving.get(&pane).map(|moving| moving.notice.as_str())
    }

    pub(in crate::app) fn moving_panes(&self) -> impl Iterator<Item = u64> + '_ {
        self.moving.keys().copied()
    }

    pub(in crate::app) fn cancel_move(&mut self, pane: u64) -> Option<Vec<u64>> {
        self.moving.remove(&pane).map(|moving| moving.requests)
    }

    pub(in crate::app) fn finish_move(&mut self, pane: u64, job: u64) -> Option<Vec<u64>> {
        if self.moving.get(&pane).is_none_or(|moving| moving.job != job) {
            return None;
        }
        self.cancel_move(pane)
    }

    pub(in crate::app) fn wait(&mut self, pane: u64, waiting: Waiting) {
        self.waiting.insert(pane, waiting);
    }

    pub(in crate::app) fn waiting(&self, pane: u64) -> Option<&Waiting> {
        self.waiting.get(&pane)
    }

    pub(in crate::app) fn waiting_panes(&self) -> impl Iterator<Item = u64> + '_ {
        self.waiting.keys().copied()
    }

    pub(in crate::app) fn all_waiting(&self) -> Vec<(u64, Waiting)> {
        self.waiting.iter().map(|(pane, waiting)| (*pane, waiting.clone())).collect()
    }

    pub(in crate::app) fn stop_waiting(&mut self, pane: u64) -> bool {
        self.waiting.remove(&pane).is_some()
    }

    pub(in crate::app) fn forget_to(&mut self, kind: Known, account: &str) {
        self.waiting.retain(|pane, waiting| {
            let keep = waiting.kind != kind || waiting.to != account;
            if !keep {
                log::info!("accounts", "a waiting restart was dropped: its account was removed", pane = *pane);
            }
            keep
        });
    }

    pub(in crate::app) fn holds(&self, pane: u64) -> bool {
        self.waiting.contains_key(&pane) || self.moving.contains_key(&pane)
    }
}
