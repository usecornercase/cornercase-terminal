use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::idle_restart::INPUT_HOLDS;
use super::wait::Condition;
use super::{Asker, Handled, Pending, Stage, Waits, conflict, json};
use crate::agents::launch::Launch;
use crate::app::App;
use crate::control::{HeldAction, HeldInfo, HeldRequest};
use crate::log;
use crate::project::Hierarchy;
use crate::term::typing::DRAFT_FOR;

pub(in crate::app) struct Held {
    pub(super) text: Option<String>,
    pub(super) enter: bool,
    pub(super) wait: Option<Condition>,
    pub(super) from: Option<u64>,
    pub(super) force: bool,
    pub(super) pressed: bool,
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

pub(super) fn pasted(pane: u64, text: bool) -> String {
    let head = format!("the person in pane {pane} had a draft in its input when they chose send now, so");
    if text {
        format!(
            "{head} the message was pasted after their draft without its Enter: it goes out when they send their \
             line; do not send it again, `cornercase read --pane {pane}` shows it"
        )
    } else {
        format!("{head} the Enter was left to them and nothing was pressed")
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
        self.agents.launches.of_pane(pane).any(Launch::submits)
    }

    pub(in crate::app) fn held_sends(&self, pane: u64) -> usize {
        self.held(pane).count()
    }

    pub(super) fn held_info(&self, pane: u64) -> HeldInfo {
        let acked = self.clients.requests.queued.get(&pane).copied();
        HeldInfo {
            pane,
            held_sends: u32::try_from(self.held_sends(pane)).unwrap_or(u32::MAX),
            held_from: self.held(pane).next().and_then(|first| first.from),
            held_queued: self.held_keys(pane).max().zip(acked).is_some_and(|(newest, acked)| acked >= newest),
            after_draft: false,
        }
    }

    pub(in crate::app) fn held_label(&self, pane: u64) -> Option<(String, bool)> {
        let count = self.held_sends(pane);
        let HeldInfo { held_from, held_queued, .. } = self.held_info(pane);
        (count > 0).then(|| (label(count, held_from, held_queued), held_queued))
    }

    pub(super) fn held_request(&mut self, asker: Asker, request: &HeldRequest) -> Handled {
        let pane = self.target(asker, request.pane, request.tab)?;
        if self.held_sends(pane) == 0 {
            return Err(conflict(format!(
                "no message is held in pane {pane}; `cornercase status` shows `held_sends` for each pane"
            )));
        }
        let after_draft = match request.action {
            HeldAction::SendNow if self.resume_holds(pane) => {
                return Err(conflict(format!(
                    "pane {pane} is restarting or resuming its agent, and the message would reach its shell or its \
                     start-up instead; try again once `cornercase status` no longer shows it resuming"
                )));
            }
            HeldAction::SendNow => self.send_held_now(pane),
            HeldAction::WaitTurn => {
                self.queue_held(pane);
                false
            }
        };
        Ok(Some(json(&HeldInfo { after_draft, ..self.held_info(pane) })))
    }

    fn held_keys(&self, pane: u64) -> impl Iterator<Item = u64> {
        let held = self.clients.requests.pending.iter().filter(move |p| p.done.ids.pane == Some(pane));
        held.filter(|p| matches!(p.stage, Stage::Held(_))).map(|p| p.key)
    }

    pub(in crate::app) fn queue_held(&mut self, pane: u64) {
        if let Some(newest) = self.held_keys(pane).max() {
            log::info!("control", "held sends acknowledged", pane = pane);
            self.clients.requests.queued.insert(pane, newest);
        }
    }

    pub(in crate::app) fn held_labels(&self) -> HashMap<u64, (String, bool)> {
        let panes = self.clients.requests.pending.iter().filter(|p| matches!(p.stage, Stage::Held(_)));
        let panes: HashSet<u64> = panes.filter_map(|p| p.done.ids.pane).collect();
        panes.into_iter().filter_map(|pane| Some((pane, self.held_label(pane)?))).collect()
    }

    fn held(&self, pane: u64) -> impl Iterator<Item = &Held> {
        self.clients.requests.pending.iter().filter(move |p| p.done.ids.pane == Some(pane)).filter_map(|p| {
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
        } else if term.typing.recent(now, self.clients.typing_holds) {
            Some(Why::Typing)
        } else {
            self.sending(pane).then_some(Why::Earlier)
        }
    }

    pub(super) fn holding_back(&self, pane: u64, now: Instant) -> Option<Why> {
        self.blocked(pane, now).or_else(|| (self.held_sends(pane) > 0).then_some(Why::Earlier))
    }

    pub(in crate::app) fn person_holds(&self, pane: u64, now: Instant) -> bool {
        self.holding_back(pane, now).is_some()
    }

    pub(super) fn hold(&mut self, client: u64, pane: u64, waits: Waits, now: Instant) -> Option<serde_json::Value> {
        let why = self.holding_back(pane, now).map_or("earlier", Why::name);
        log::info!("control", "send held", pane = pane, why = why, waiting = self.held_sends(pane) + 1);
        let key = self.clients.requests.key();
        self.clients.requests.push(client, key, waits);
        None
    }

    pub(in crate::app) fn settle_drafts(&mut self, now: Instant) {
        for term in self.projects.terms_mut() {
            term.settle_draft(now);
        }
    }

    pub(super) fn release_held(&mut self, now: Instant) -> Vec<u64> {
        let held: HashSet<u64> = self
            .clients
            .requests
            .pending
            .iter()
            .filter(|p| matches!(p.stage, Stage::Held(_)))
            .filter_map(|p| p.done.ids.pane)
            .collect();
        self.clients.requests.queued.retain(|pane, _| held.contains(pane));
        let mut seen = HashSet::new();
        let mut over = Vec::new();
        let mut i = 0;
        while let Some(pending) = self.clients.requests.pending.get(i) {
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
            let pending = self.clients.requests.pending.remove(i);
            if let Some(kept) = self.release(pending, gone, now, &mut over) {
                self.clients.requests.pending.insert(i, kept);
                i += 1;
            }
        }
        over
    }

    fn release(&mut self, mut pending: Pending, gone: bool, now: Instant, over: &mut Vec<u64>) -> Option<Pending> {
        let pane = pending.done.ids.pane.unwrap_or_default();
        if gone {
            self.answer(pending.client, Err(format!("pane {pane} closed before the message went out")));
            return None;
        }
        let Stage::Held(held) = std::mem::replace(&mut pending.stage, Stage::Reading) else { return Some(pending) };
        let mut held = *held;
        let over_draft = held.pressed && self.projects.pane_by(pane).is_some_and(|term| term.draft(now));
        log::info!("control", "send released", pane = pane, forced = held.force, after_draft = over_draft);
        if let Err(refusal) = self.refuse_send(pane, held.force) {
            self.refuse_or_answer(pending.client, Err(refusal));
            return None;
        }
        let (had_text, asked_enter) = (held.text.is_some(), held.enter);
        if over_draft {
            over.push(pane);
            self.leave_enter(pane, &mut held);
        }
        match self.put_through(pane, held, now) {
            Err(refusal) => self.refuse_or_answer(pending.client, Err(refusal)),
            Ok(None) if over_draft && asked_enter => {
                self.answer(pending.client, Err(pasted(pane, had_text)));
            }
            Ok(None) => self.answer(pending.client, Ok(json(&pending.done))),
            Ok(Some((mut launch, stage))) => {
                launch.key = Some(pending.key);
                self.agents.launches.add(launch);
                pending.stage = stage;
                return Some(pending);
            }
        }
        None
    }

    fn leave_enter(&mut self, pane: u64, held: &mut Held) {
        if let Some(text) = &held.text
            && let Some(term) = self.projects.pane_by_mut(pane)
        {
            term.typed_text(text);
        }
        held.wait = None;
        held.enter = false;
    }

    pub(in crate::app) fn send_held_now(&mut self, pane: u64) -> bool {
        let now = Instant::now();
        if let Some(term) = self.projects.pane_by_mut(pane) {
            term.settle_draft(now);
        }
        let first =
            self.clients.requests.pending.iter_mut().filter(|p| p.done.ids.pane == Some(pane)).find_map(
                |p| match &mut p.stage {
                    Stage::Held(held) => Some(held),
                    _ => None,
                },
            );
        let Some(held) = first else { return false };
        held.force = true;
        held.pressed = true;
        self.release_held(now).contains(&pane)
    }

    pub(super) fn held_due(&self, pane: u64) -> Vec<Instant> {
        let at = self.projects.pane_by(pane).and_then(|term| term.typing.at);
        at.into_iter()
            .flat_map(|at| [at.checked_add(self.clients.typing_holds), at.checked_add(DRAFT_FOR)])
            .flatten()
            .collect()
    }
}

#[cfg(test)]
mod tests;
