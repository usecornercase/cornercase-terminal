use std::iter::once;
use std::time::{Instant, SystemTime};

use super::machines::{Pick, failed, stale, unread};
use super::{at, here, sections};
use crate::agents::usage::{self, Severity};
use crate::app::App;
use crate::control::usage::{LimitInfo, MachineRead, PlanInfo, Plans};
use crate::machines::HERE;
use crate::time::{self, Stamp};
use crate::ui;

#[derive(Debug, Default)]
pub(super) struct Shelf {
    pub(super) name: String,
    pub(super) status: String,
    pub(super) error: Option<String>,
    pub(super) rows: Vec<PlanInfo>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Row {
    pub(super) info: PlanInfo,
    pub(super) on: Vec<String>,
}

impl Row {
    fn joins(&self, name: &str, info: &PlanInfo) -> bool {
        info.identity.is_some() && self.info.identity == info.identity && !self.on.iter().any(|on| on == name)
    }

    fn join(&mut self, name: &str, info: &PlanInfo) {
        self.on.push(name.to_string());
        if info.age.is_some_and(|age| self.info.age.is_none_or(|kept| age < kept)) {
            let (account, label, active) = (self.info.account.take(), self.info.label.take(), self.info.active);
            self.info = PlanInfo { account, label, active, ..info.clone() };
        }
    }

    fn section(&self, clock: i64) -> ui::UsageSection {
        let mut section = section(&self.info, clock);
        section.on = (self.on.len() > 1).then(|| format!("on {}", self.on.join(", ")));
        section
    }
}

impl App {
    pub(super) fn picked_limits(&self, now: Instant, clock: i64) -> (Vec<ui::UsageSection>, Vec<ui::UsageMachine>) {
        let (state, config) = (&self.usages.state, &self.config);
        let shelves: Vec<Shelf> = match self.usage_pick() {
            Pick::Here => return (sections(state, config, now, clock), Vec::new()),
            Pick::Machine(name) => self.shelves(clock).filter(|shelf| shelf.name == name).collect(),
            Pick::All => once(Shelf { name: HERE.into(), rows: here(state, config, now, clock), ..Shelf::default() })
                .chain(self.shelves(clock))
                .collect(),
        };
        (Vec::new(), shown(&shelves, clock))
    }

    fn shelves(&self, clock: i64) -> impl Iterator<Item = Shelf> {
        let now = at(clock);
        self.machine_reads::<Plans>().into_iter().map(move |(machine, plans)| shelf(&machine, plans, now))
    }
}

fn shelf(machine: &MachineRead, plans: Option<Plans>, now: SystemTime) -> Shelf {
    let read = machine.read_at.as_deref().and_then(Stamp::parse);
    let elapsed = read.and_then(|at| now.duration_since(at).ok()).map_or(0, |since| since.as_secs());
    let rows = plans.map(|plans| plans.agents).unwrap_or_default();
    Shelf {
        name: machine.name.clone(),
        status: stale(machine, now).unwrap_or_default(),
        error: unread(machine).map(str::to_string).or_else(|| failed(machine, now)),
        rows: rows.into_iter().map(|info| PlanInfo { age: info.age.map(|age| age + elapsed), ..info }).collect(),
    }
}

fn shown(shelves: &[Shelf], clock: i64) -> Vec<ui::UsageMachine> {
    shelves
        .iter()
        .zip(merge(shelves))
        .map(|(shelf, rows)| ui::UsageMachine {
            name: shelf.name.clone(),
            status: shelf.status.clone(),
            error: shelf.error.clone(),
            sections: rows.iter().map(|row| row.section(clock)).collect(),
        })
        .collect()
}

pub(super) fn merge(shelves: &[Shelf]) -> Vec<Vec<Row>> {
    let mut placed: Vec<Vec<Row>> = Vec::new();
    for shelf in shelves {
        let kept = shelve(&mut placed, shelf);
        placed.push(kept);
    }
    placed
}

fn shelve(placed: &mut [Vec<Row>], shelf: &Shelf) -> Vec<Row> {
    let mut kept = Vec::new();
    for info in &shelf.rows {
        match placed.iter_mut().flatten().find(|row| row.joins(&shelf.name, info)) {
            Some(row) => row.join(&shelf.name, info),
            None => kept.push(Row { info: info.clone(), on: vec![shelf.name.clone()] }),
        }
    }
    kept
}

pub(super) fn section(info: &PlanInfo, clock: i64) -> ui::UsageSection {
    let plan = info.plan.as_deref().map_or_else(String::new, |plan| format!(" · {plan} plan"));
    let status = if info.loading { "loading…".into() } else { info.age.map(usage::updated).unwrap_or_default() };
    ui::UsageSection {
        title: format!("{}{plan}", info.heading()),
        status,
        on: None,
        error: info.error.as_ref().map(|error| format!("usage unavailable: {error}")),
        windows: info.limits.iter().map(|limit| window(limit, clock)).collect(),
        empty: info.notice.clone(),
        extra: info.extra.clone(),
    }
}

fn window(limit: &LimitInfo, clock: i64) -> ui::UsageWindow {
    let resets = limit.resets_at.as_deref().and_then(Stamp::parse).map(|at| usage::until(time::secs(at) - clock));
    ui::UsageWindow {
        label: limit.label.clone(),
        percent: limit.percent,
        severity: Severity::parse(Some(&limit.severity), limit.percent),
        resets: resets.unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests;
