use std::collections::BTreeMap;

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use super::{Action, Focus, Start};
use crate::accounts::AgentAccounts;
use crate::machines::run_on::{self, RunOn, Target};

const LOST: &str = "the machine";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts read from the machine: cloned, listed, accounted, tracked"
)]
pub(crate) struct There {
    pub(crate) place: String,
    pub(crate) cloned: bool,
    pub(crate) listed: bool,
    pub(crate) accounted: bool,
    pub(crate) tracked: bool,
    pub(crate) lacking: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) era: u64,
}

impl Start {
    pub(crate) fn with_run_on(mut self, run_on: Option<RunOn>) -> Self {
        self.run_on = run_on;
        self
    }

    pub(crate) fn machine(&self) -> Option<&Target> {
        self.run_on.as_ref()?.machine()
    }

    pub(super) fn elsewhere(&self) -> bool {
        self.run_on.as_ref().is_some_and(RunOn::elsewhere)
    }

    pub(crate) fn there(&self) -> Option<&There> {
        self.there.as_ref().filter(|_| self.machine().is_some())
    }

    pub(crate) fn choose_machine(&mut self, at: Option<usize>) -> bool {
        let Some(run_on) = &mut self.run_on else { return false };
        if run_on.is_chosen(at) || !run_on.choose(at) {
            return false;
        }
        self.there = None;
        self.notice = None;
        self.picked_there.clear();
        self.close_accounts();
        self.leave_prompt();
        true
    }

    pub(crate) fn pick_there(&mut self, picked: BTreeMap<String, String>) {
        self.picked_there = picked;
    }

    pub(crate) fn read_accounts_there(&mut self, accounts: Vec<AgentAccounts>) {
        let mut kept: BTreeMap<String, String> =
            self.accounts.iter().map(|agent| (agent.kind.clone(), agent.chosen().to_string())).collect();
        kept.extend(std::mem::take(&mut self.picked_there));
        self.accounts = accounts;
        self.seed_accounts(&kept);
    }

    pub(crate) fn templated(&self) -> Option<String> {
        if self.asked.is_none() || !self.elsewhere() {
            return None;
        }
        Some(run_on::from_template(self.machine().map_or(LOST, |target| target.name.as_str())))
    }

    pub(crate) fn leave_prompt(&mut self) {
        if self.focus == Focus::Prompt && self.templated().is_some() {
            self.focus = Focus::Branch;
        }
    }

    pub(crate) fn run_on_spans(&self, dim: Style) -> Option<Vec<Span<'static>>> {
        let mut spans = self.run_on.as_ref()?.spans(dim);
        if let Some(missing) = self.there().and_then(|there| there.lacking.clone()) {
            spans.push(Span::styled(format!(" · {missing}"), Style::default().fg(Color::Red)));
        }
        Some(spans)
    }

    pub(crate) fn run_on_choices(&self) -> usize {
        self.run_on.as_ref().map_or(0, |run_on| run_on.targets.len() + 1)
    }

    pub(super) fn heading_there(&self, name: &str) -> Option<String> {
        if self.run_on.as_ref().is_some_and(|run_on| run_on.lost.is_some()) {
            return Some(format!("{name} · choose where it runs"));
        }
        let target = self.machine()?;
        let place = self.there().map_or_else(|| format!("on {}", target.name), |there| there.place.clone());
        Some(format!("{name} {place}"))
    }

    pub(super) fn not_ready_there(&mut self) -> Option<Action> {
        if let Some(lost) = self.run_on.as_ref().and_then(|run_on| run_on.lost.clone()) {
            self.notice = Some(lost);
            return Some(Action::None);
        }
        let name = self.machine()?.name.clone();
        let Some(there) = self.there() else {
            self.notice = Some(format!("waiting for {name} to answer"));
            return Some(Action::None);
        };
        if !there.cloned {
            return Some(Action::CloneThere);
        }
        let (lacking, listed, accounted, tracked) =
            (there.lacking.clone(), there.listed, there.accounted, there.tracked);
        if let Some(lacking) = lacking {
            self.notice = Some(lacking);
            return Some(Action::AskAgain);
        }
        let reading = match (listed, accounted, tracked) {
            (false, _, _) => "reading the flows",
            (true, false, _) => "reading the accounts",
            (true, true, false) => "checking the tracker",
            (true, true, true) => return None,
        };
        self.notice = Some(format!("{reading} on {name}…"));
        Some(Action::None)
    }

    pub(crate) fn starting_there(&self) -> Option<String> {
        let target = self.machine().filter(|_| self.waiting)?;
        Some(format!("starting on {}…", target.name))
    }
}

#[cfg(test)]
mod tests;
