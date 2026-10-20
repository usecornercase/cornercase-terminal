use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::idle_restart::INPUT_HOLDS;
use super::wait::Condition;
use super::{Pending, Stage, Waits, json};
use crate::app::App;
use crate::log;
use crate::project::Hierarchy;
use crate::term::typing::DRAFT_FOR;

pub(in crate::app) struct Held {
    pub(super) text: Option<String>,
    pub(super) enter: bool,
    pub(super) wait: Option<Condition>,
    pub(super) from: Option<u64>,
    pub(super) force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Why {
    Draft,
    Typing,
    Earlier,
}

impl Why {
    fn name(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Typing => "typing",
            Self::Earlier => "earlier",
        }
    }
}

pub(super) fn waiting(pane: u64, why: Option<Why>) -> String {
    match why {
        Some(Why::Draft) => format!(
            "the person in pane {pane} has a draft in its input they have not sent yet, so nothing was sent; it goes \
             out once they send or clear it"
        ),
        Some(Why::Typing) => {
            format!("the person in pane {pane} typed in the last {}s, so nothing was sent", INPUT_HOLDS.as_secs())
        }
        Some(Why::Earlier) | None => {
            format!("an earlier message to pane {pane} has not gone out yet, so nothing was sent")
        }
    }
}

pub(crate) fn label(count: usize, from: Option<u64>, queued: bool) -> String {
    if queued {
        return match count {
            1 => "queued: goes right after yours".to_string(),
            n => format!("{n} queued: the first goes right after yours"),
        };
    }
    let from = from.map(|pane| format!(" from pane {pane}")).unwrap_or_default();
    let waiting = match count {
        1 => format!("1 message waiting{from}"),
        n if from.is_empty() => format!("{n} messages waiting"),
        n => format!("{n} messages waiting, the first{from}"),
    };
    format!("{waiting} · goes after yours")
}

impl App {
    pub(in crate::app) fn sending(&self, pane: u64) -> bool {
        self.agents.launches.iter().any(|launch| launch.term == pane && launch.submits())
    }

    pub(in crate::app) fn held_sends(&self, pane: u64) -> usize {
        self.held(pane).count()
    }

    pub(in crate::app) fn held_label(&self, pane: u64) -> Option<(String, bool)> {
        let first = self.held(pane).next()?;
        let newest = self.held_keys(pane).max()?;
        let queued = self.wire.requests.queued.get(&pane).is_some_and(|acked| *acked >= newest);
        Some((label(self.held_sends(pane), first.from, queued), queued))
    }

    fn held_keys(&self, pane: u64) -> impl Iterator<Item = u64> {
        let held = self.wire.requests.pending.iter().filter(move |p| p.done.ids.pane == Some(pane));
        held.filter(|p| matches!(p.stage, Stage::Held(_))).map(|p| p.key)
    }

    pub(in crate::app) fn queue_held(&mut self, pane: u64) {
        if let Some(newest) = self.held_keys(pane).max() {
            log::info!("control", "held sends acknowledged", pane = pane);
            self.wire.requests.queued.insert(pane, newest);
        }
    }

    pub(in crate::app) fn held_labels(&self) -> HashMap<u64, (String, bool)> {
        let panes = self.wire.requests.pending.iter().filter(|p| matches!(p.stage, Stage::Held(_)));
        let panes: HashSet<u64> = panes.filter_map(|p| p.done.ids.pane).collect();
        panes.into_iter().filter_map(|pane| Some((pane, self.held_label(pane)?))).collect()
    }

    fn held(&self, pane: u64) -> impl Iterator<Item = &Held> {
        self.wire.requests.pending.iter().filter(move |p| p.done.ids.pane == Some(pane)).filter_map(|p| {
            match &p.stage {
                Stage::Held(held) => Some(held.as_ref()),
                _ => None,
            }
        })
    }

    pub(super) fn blocked(&self, pane: u64, now: Instant) -> Option<Why> {
        let term = self.projects.pane_by(pane)?;
        if term.draft(now) {
            Some(Why::Draft)
        } else if term.typing.recent(now, self.wire.typing_holds) {
            Some(Why::Typing)
        } else {
            self.sending(pane).then_some(Why::Earlier)
        }
    }

    pub(super) fn holding_back(&self, pane: u64, now: Instant) -> Option<Why> {
        self.blocked(pane, now).or_else(|| (self.held_sends(pane) > 0).then_some(Why::Earlier))
    }

    pub(super) fn hold(&mut self, client: u64, pane: u64, waits: Waits, now: Instant) -> Option<serde_json::Value> {
        let why = self.holding_back(pane, now).map_or("earlier", Why::name);
        log::info!("control", "send held", pane = pane, why = why, waiting = self.held_sends(pane) + 1);
        let key = self.wire.requests.key();
        self.wire.requests.push(client, key, waits);
        None
    }

    pub(in crate::app) fn settle_drafts(&mut self, now: Instant) {
        for term in self.projects.terms_mut() {
            term.settle_draft(now);
        }
    }

    pub(super) fn release_held(&mut self, now: Instant) {
        let held: HashSet<u64> = self
            .wire
            .requests
            .pending
            .iter()
            .filter(|p| matches!(p.stage, Stage::Held(_)))
            .filter_map(|p| p.done.ids.pane)
            .collect();
        self.wire.requests.queued.retain(|pane, _| held.contains(pane));
        let mut seen = HashSet::new();
        let mut i = 0;
        while let Some(pending) = self.wire.requests.pending.get(i) {
            let Stage::Held(held) = &pending.stage else {
                i += 1;
                continue;
            };
            let pane = pending.done.ids.pane.unwrap_or_default();
            let gone = self.projects.pane_by(pane).is_none();
            let blocked = if held.force { self.sending(pane).then_some(Why::Earlier) } else { self.blocked(pane, now) };
            if (!held.force && !seen.insert(pane)) || (blocked.is_some() && !gone) {
                i += 1;
                continue;
            }
            let pending = self.wire.requests.pending.remove(i);
            if let Some(kept) = self.release(pending, gone, now) {
                self.wire.requests.pending.insert(i, kept);
                i += 1;
            }
        }
    }

    fn release(&mut self, mut pending: Pending, gone: bool, now: Instant) -> Option<Pending> {
        let pane = pending.done.ids.pane.unwrap_or_default();
        if gone {
            self.answer(pending.client, Err(format!("pane {pane} closed before the message went out")));
            return None;
        }
        let Stage::Held(held) = std::mem::replace(&mut pending.stage, Stage::Reading) else { return Some(pending) };
        log::info!("control", "send released", pane = pane, forced = held.force);
        let delivered = self.refuse_send(pane, held.force).and_then(|()| self.put_through(pane, *held, now));
        match delivered {
            Err(refusal) => self.refuse_or_answer(pending.client, Err(refusal)),
            Ok(None) => self.answer(pending.client, Ok(json(&pending.done))),
            Ok(Some((mut launch, stage))) => {
                launch.key = Some(pending.key);
                self.agents.launches.push(launch);
                pending.stage = stage;
                return Some(pending);
            }
        }
        None
    }

    pub(in crate::app) fn send_held_now(&mut self, pane: u64) {
        let first =
            self.wire.requests.pending.iter_mut().filter(|p| p.done.ids.pane == Some(pane)).find_map(|p| match &mut p
                .stage
            {
                Stage::Held(held) => Some(held),
                _ => None,
            });
        if let Some(held) = first {
            held.force = true;
            self.release_held(Instant::now());
        }
    }

    pub(super) fn held_due(&self, pane: u64) -> Vec<Instant> {
        let at = self.projects.pane_by(pane).and_then(|term| term.typing.at);
        at.into_iter()
            .flat_map(|at| [at.checked_add(self.wire.typing_holds), at.checked_add(DRAFT_FOR)])
            .flatten()
            .collect()
    }
}

#[cfg(test)]
mod tests;
