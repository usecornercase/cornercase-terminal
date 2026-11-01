use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::App;
use super::control::json;
use super::event::AppEvent;
use super::machines::{Held, MachineEvent, OLDER};
use crate::agents::stats::View;
use crate::control::contract::{Code, Refusal};
use crate::control::usage::{MachineRead, MachineStats, Plans};
use crate::control::{Command, Response, Stats, Usage};
use crate::log::{Job, Level};
use crate::machines::link::Down;
use crate::machines::{self, State, ask};
use crate::time::Stamp;

const STATS: Duration = Duration::from_secs(15);
const SLACK: Duration = Duration::from_secs(15);
pub(super) const UNREADABLE: &str = "its answer could not be read";
pub(super) const GONE: &str = "the machine was removed before it answered";
pub(super) const CONNECTING: &str = "still connecting";
pub(super) const NOT_READ: &str = "not read yet";

pub(super) struct Cached<T> {
    pub(super) read: Option<(T, SystemTime)>,
    pub(super) error: Option<String>,
    pub(super) asking: bool,
}

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Self { read: None, error: None, asking: false }
    }
}

#[derive(Default)]
pub(super) struct Cache {
    pub(super) stats: Cached<View>,
    pub(super) usage: Cached<Plans>,
}

pub(super) trait Asked: DeserializeOwned + Clone + Send + 'static {
    const WHAT: &'static str;
    fn command() -> Command;
    fn timeout(app: &App) -> Duration;
    fn kept(cache: &Cache) -> &Cached<Self>;
    fn cached(cache: &mut Cache) -> &mut Cached<Self>;
    fn event(link: u64, result: Result<Self, String>) -> MachineEvent;
    fn read(value: Value, now: SystemTime) -> serde_json::Result<Self>;
    fn one(self, machine: MachineRead) -> Value;
}

impl Asked for View {
    const WHAT: &'static str = "stats";

    fn command() -> Command {
        Command::Stats(Stats::default())
    }

    fn timeout(_: &App) -> Duration {
        STATS
    }

    fn kept(cache: &Cache) -> &Cached<Self> {
        &cache.stats
    }

    fn cached(cache: &mut Cache) -> &mut Cached<Self> {
        &mut cache.stats
    }

    fn event(link: u64, result: Result<Self, String>) -> MachineEvent {
        MachineEvent::Stats { link, result }
    }

    fn read(value: Value, now: SystemTime) -> serde_json::Result<Self> {
        serde_json::from_value::<Self>(value).map(|view| view.settled(now))
    }

    fn one(self, machine: MachineRead) -> Value {
        json(&MachineStats { view: self, machine: Some(machine), machines: Vec::new() })
    }
}

impl Asked for Plans {
    const WHAT: &'static str = "usage";

    fn command() -> Command {
        Command::Usage(Usage::default())
    }

    fn timeout(app: &App) -> Duration {
        app.usages.timeout() + SLACK
    }

    fn kept(cache: &Cache) -> &Cached<Self> {
        &cache.usage
    }

    fn cached(cache: &mut Cache) -> &mut Cached<Self> {
        &mut cache.usage
    }

    fn event(link: u64, result: Result<Self, String>) -> MachineEvent {
        MachineEvent::Usage { link, result }
    }

    fn read(value: Value, _: SystemTime) -> serde_json::Result<Self> {
        serde_json::from_value(value)
    }

    fn one(self, machine: MachineRead) -> Value {
        json(&Self { machine: Some(machine), ..self })
    }
}

impl Held {
    pub(super) fn another_computer(&self) -> bool {
        !matches!(self.status.down, Some(Down::ThisComputer | Down::SameServer(_)))
    }
}

impl App {
    pub(super) fn ask_machine<T: Asked>(&mut self, name: &str) -> Option<u64> {
        let timeout = T::timeout(self);
        let held = self.machines.named(name)?;
        let link = held.link.id();
        let cached = T::cached(&mut held.cache);
        if cached.asking {
            return Some(link);
        }
        if held.status.state != State::Linked {
            return None;
        }
        cached.asking = true;
        let (name, destination) = (held.name.clone(), held.destination.clone());
        let connect = self.machines.connector(&destination);
        let job = Job::new(Level::Info, "machines", T::WHAT).with("name", &name).slow_after(timeout);
        let there = name.clone();
        let work = move || Ok(ask::ask(connect.as_ref(), &there, T::command(), timeout));
        self.spawn_job(job, work, move |result, job| {
            let result = heard::<T>(result).map_err(|why| machines::hide_destination(&why, &name, &destination));
            if let Err(why) = &result {
                job.fail(why);
            }
            Some(AppEvent::Machine(T::event(link, result)))
        });
        Some(link)
    }

    pub(super) fn machine_answered<T: Asked>(&mut self, link: u64, result: Result<T, String>) {
        let Some(held) = self.machines.by_link(link) else { return };
        let another = held.another_computer();
        let cached = T::cached(&mut held.cache);
        cached.asking = false;
        match result {
            Ok(value) if another => {
                cached.read = Some((value, SystemTime::now()));
                cached.error = None;
            }
            Ok(_) => {}
            Err(why) => cached.error = Some(why),
        }
    }

    pub(super) fn ask_every_machine<T: Asked>(&mut self) -> Vec<u64> {
        let names: Vec<String> = self.machines_in_order().map(|held| held.name.clone()).collect();
        names.iter().filter_map(|name| self.ask_machine::<T>(name)).collect()
    }

    pub(super) fn machines_answered<T: Asked>(&self, links: &[u64]) -> bool {
        let held = self.machines.held();
        let answered = |link: &u64| {
            held.iter().find(|held| held.link.id() == *link).is_none_or(|held| !T::kept(&held.cache).asking)
        };
        links.iter().all(answered)
    }

    pub(super) fn machine_reads<T: Asked>(&self) -> Vec<(MachineRead, Option<T>)> {
        self.machines_in_order().map(|held| self.machine_read(held)).collect()
    }

    pub(super) fn machine_answer<T: Asked>(&self, link: u64) -> Result<Value, Refusal> {
        let held = self.machines.held().iter().find(|held| held.link.id() == link);
        let Some(held) = held else { return Err(Refusal::new(Code::Failed, GONE)) };
        match self.machine_read::<T>(held) {
            (machine, Some(value)) => Ok(value.one(machine)),
            (machine, None) => Err(Refusal::new(Code::Failed, unread(T::WHAT, &machine))),
        }
    }

    fn machines_in_order(&self) -> impl Iterator<Item = &Held> {
        let held = self.machines.held();
        let listed = machines::accepted(&self.config.machines);
        listed.filter_map(move |machine| held.iter().find(|held| held.destination == machine.destination))
    }

    fn machine_read<T: Asked>(&self, held: &Held) -> (MachineRead, Option<T>) {
        let cached = T::kept(&held.cache);
        let read = cached.read.as_ref().filter(|_| held.another_computer());
        let hidden = |why: String| machines::hide_destination(&why, &held.name, &held.destination);
        let error = self.machines.reason(held).map(hidden).or_else(|| cached.error.clone());
        let machine = MachineRead {
            name: held.name.clone(),
            state: held.status.state,
            seen: held.status.seen.and_then(stamp),
            read_at: read.map(|(_, at)| Stamp(*at).to_string()),
            error: error.or_else(|| read.is_none().then(|| unread_reason(held.status.state).into())),
        };
        (machine, read.map(|(value, _)| value.clone()))
    }
}

fn stamp(secs: i64) -> Option<String> {
    let secs = u64::try_from(secs).ok()?;
    Some(Stamp(UNIX_EPOCH + Duration::from_secs(secs)).to_string())
}

fn unread(what: &str, machine: &MachineRead) -> String {
    format!("no {what} from `{}`: {}", machine.name, machine.error.as_deref().unwrap_or(NOT_READ))
}

fn unread_reason(state: State) -> &'static str {
    match state {
        State::Connecting => CONNECTING,
        State::Older => OLDER,
        State::Linked | State::Unreachable => NOT_READ,
    }
}

fn heard<T: Asked>(result: crate::error::Result<Result<String, String>>) -> Result<T, String> {
    let text = result.map_err(|error| error.to_string())??;
    match serde_json::from_str::<Response>(&text) {
        Ok(Response::Ok(value)) => T::read(value, SystemTime::now()).map_err(|_| UNREADABLE.into()),
        Ok(Response::Error(why)) => Err(why),
        Err(_) => Err(UNREADABLE.into()),
    }
}
