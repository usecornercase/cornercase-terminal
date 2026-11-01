use std::iter::once;
use std::time::SystemTime;

use crate::agents::stats::{PEAK, View};
use crate::agents::usage::Page;
use crate::app::App;
use crate::control::usage::{MachineRead, Plans};
use crate::machines::{EVERY, HERE, NOT_SEEN, State};
use crate::time::{self, Stamp};

const NOT_SUMMED: &str = "not summed";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Pick {
    Here,
    Machine(String),
    #[default]
    All,
}

impl App {
    fn machine_names(&self) -> Vec<String> {
        self.machines_in_order().map(|held| held.name.clone()).collect()
    }

    pub(super) fn usage_pick(&self) -> Pick {
        let names = self.machine_names();
        match &self.usages.machine {
            _ if names.is_empty() => Pick::Here,
            Pick::Machine(name) if !names.contains(name) => Pick::All,
            pick => pick.clone(),
        }
    }

    pub(super) fn usage_picks(&self) -> (Vec<String>, usize) {
        let names = self.machine_names();
        if names.is_empty() {
            return (Vec::new(), 0);
        }
        let picked = match self.usage_pick() {
            Pick::Here => 0,
            Pick::Machine(name) => names.iter().position(|named| *named == name).map_or(0, |i| i + 1),
            Pick::All => names.len() + 1,
        };
        (once(HERE.to_string()).chain(names).chain(once(EVERY.to_string())).collect(), picked)
    }

    pub(super) fn pick_usage(&mut self, i: usize) {
        let names = self.machine_names();
        let pick = match i.checked_sub(1) {
            None => Pick::Here,
            Some(i) => names.get(i).map_or(Pick::All, |name| Pick::Machine(name.clone())),
        };
        if pick != self.usage_pick() {
            self.usages.machine = pick;
            self.ask_usage();
        }
    }

    pub(super) fn ask_machines(&mut self) {
        let stats = self.usages.page == Page::Stats;
        match self.usage_pick() {
            Pick::Here => {}
            Pick::Machine(name) if stats => {
                self.ask_machine::<View>(&name);
            }
            Pick::Machine(name) => {
                self.ask_machine::<Plans>(&name);
            }
            Pick::All if stats => {
                self.ask_every_machine::<View>();
            }
            Pick::All => {
                self.ask_every_machine::<Plans>();
            }
        }
    }

    pub(super) fn picked_stats(&self, now: SystemTime) -> (View, Vec<String>) {
        match self.usage_pick() {
            Pick::Here => (self.stats_view(now), Vec::new()),
            Pick::Machine(name) => self.one_machine_stats(&name, now),
            Pick::All => {
                let (view, machines) = self.summed_stats(now);
                let notes = machines.iter().filter_map(|machine| note(machine, now, true));
                (view, once(PEAK.to_string()).chain(notes).collect())
            }
        }
    }

    fn one_machine_stats(&self, name: &str, now: SystemTime) -> (View, Vec<String>) {
        let Some((view, machine)) = self.machine_stats(name, now) else { return (self.stats_view(now), Vec::new()) };
        (view.unwrap_or_else(|| View::default().settled(now)), note(&machine, now, false).into_iter().collect())
    }
}

fn note(machine: &MachineRead, now: SystemTime, all: bool) -> Option<String> {
    let name = &machine.name;
    if let Some(why) = unread(machine) {
        return Some(if all { format!("{name}: {NOT_SUMMED} · {why}") } else { format!("{name}: {why}") });
    }
    stale(machine, now).or_else(|| failed(machine, now)).map(|why| format!("{name}: {why}"))
}

pub(super) fn unread(machine: &MachineRead) -> Option<&str> {
    machine.read_at.is_none().then(|| machine.error.as_deref().unwrap_or(NOT_SEEN))
}

pub(super) fn stale(machine: &MachineRead, now: SystemTime) -> Option<String> {
    (machine.state != State::Linked).then(|| seen(machine, now))
}

pub(super) fn failed(machine: &MachineRead, now: SystemTime) -> Option<String> {
    let why = machine.error.as_deref().filter(|_| machine.state == State::Linked)?;
    Some(format!("{why} · numbers read {}", since(machine.read_at.as_deref(), now)?))
}

fn seen(machine: &MachineRead, now: SystemTime) -> String {
    since(machine.seen.as_deref(), now).map_or_else(|| NOT_SEEN.into(), |ago| format!("seen {ago}"))
}

fn since(stamp: Option<&str>, now: SystemTime) -> Option<String> {
    let at = Stamp::parse(stamp?)?;
    Some(time::ago(now.duration_since(at).unwrap_or_default()))
}
