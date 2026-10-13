use std::time::{Duration, Instant};

use super::wait::Condition;
use super::{Handled, Pending, STARTS_WITHIN, Stage, deadline};
use crate::app::{App, Toast};
use crate::control::{self, Done};
use crate::log;
use crate::project::Workspace;
use crate::term::Term;
use crate::ui;

const INPUT_HOLDS: Duration = Duration::from_secs(2);

const RESTART_CANCELLED: &str = "the restart was cancelled in the window; the server keeps running";

pub(super) fn held(panes: &[u64]) -> String {
    let ids: Vec<String> = panes.iter().map(u64::to_string).collect();
    match ids.as_slice() {
        [] => "the server keeps running".into(),
        [one] => format!("the agent in pane {one} has not ended its turn, so the server keeps running"),
        [first @ .., last] => format!(
            "the agents in panes {} and {last} have not ended their turn, so the server keeps running",
            first.join(", ")
        ),
    }
}

fn pending_restart(agents: usize) -> String {
    match agents {
        0 => "restart pending".into(),
        1 => "restart pending until 1 agent ends its turn".into(),
        n => format!("restart pending until {n} agents end their turn"),
    }
}

impl App {
    pub(super) fn restart_request(
        &mut self,
        client: u64,
        caller: Option<u64>,
        restart: &control::RestartWhenIdle,
        now: Instant,
    ) -> Handled {
        let timeout = deadline(restart.timeout, now)?;
        let key = self.requests.key();
        let stage = Stage::Restart { caller };
        self.requests.pending.push(Pending { client: Some(client), key, timeout, done: Done::default(), stage });
        self.show_pending_restart(now);
        Ok(None)
    }

    pub(super) fn holding(&self, caller: Option<u64>, now: Instant) -> Vec<u64> {
        let launching: Vec<u64> = self.launches.iter().filter(|l| !l.waits_for_you()).map(|l| l.term).collect();
        let terms = self.projects.iter().flat_map(|p| &p.workspaces).flat_map(Workspace::terms);
        let holds = |term: &&Term| Some(term.id) != caller && (launching.contains(&term.id) || self.at_work(term, now));
        terms.filter(holds).map(|term| term.id).collect()
    }

    fn at_work(&self, term: &Term, now: Instant) -> bool {
        if term.agent.status().is_some() && Condition::TurnOver.ended(&term.agent).is_none() {
            return true;
        }
        let within = |at: Instant, limit: Duration| now.saturating_duration_since(at) < limit;
        let typed = term.input_at.is_some_and(|at| within(at, INPUT_HOLDS));
        let unanswered = term.submitted.is_some_and(|at| within(at, STARTS_WITHIN) && !term.agent.reacted(at));
        (typed || unanswered) && self.runs_agent(term)
    }

    pub(in crate::app) fn show_pending_restart(&mut self, now: Instant) {
        let shown = self.toast.as_ref().is_some_and(|t| t.icon == ui::ToastIcon::Restart);
        let pending = self.requests.pending.iter().find_map(|p| match p.stage {
            Stage::Restart { caller } => Some(caller),
            _ => None,
        });
        match pending {
            None if shown => self.toast = None,
            Some(caller) if shown || self.toast.is_none() => {
                let message = pending_restart(self.holding(caller, now).len());
                self.toast = Some(Toast::new(message, ui::ToastIcon::Restart));
            }
            _ => {}
        }
    }

    pub(in crate::app) fn cancel_restarts(&mut self) {
        let (cancelled, kept): (Vec<Pending>, Vec<Pending>) = std::mem::take(&mut self.requests.pending)
            .into_iter()
            .partition(|p| matches!(p.stage, Stage::Restart { .. }));
        self.requests.pending = kept;
        log::info!("control", "restart cancelled in the window", waits = cancelled.len());
        for pending in cancelled {
            self.answer(pending.client, Err(RESTART_CANCELLED.into()));
        }
    }
}
