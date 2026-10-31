use std::sync::Arc;
use std::time::{Instant, SystemTime};

use super::App;
use super::control::json;
use super::event::AppEvent;
use super::machine_usage::Cache;
use super::overlay::Overlay;
use crate::agents::stats::View;
use crate::control;
use crate::control::contract::{Code, Refusal};
use crate::control::usage::Plans;
use crate::error::Result;
use crate::host::network;
use crate::log::{self, Job, Level};
use crate::machines::link::{self, Connect, Down, Event, Link, Report, Servers};
use crate::machines::wake::{Network, Sleep};
use crate::machines::{self, Info, Machine, State, Status};
use crate::settings::Page;

const OLDER: &str = "its cornercase is older than this one";

pub(super) type Connector = Box<dyn Fn(&str) -> Box<dyn Connect> + Send>;
pub(super) type Updater = Arc<dyn Fn(&str) -> Result<()> + Send + Sync>;

#[derive(Debug)]
pub(crate) enum MachineEvent {
    Link(Report),
    Updated { link: u64, result: std::result::Result<(), String>, hidden: std::result::Result<(), String> },
    Stats { link: u64, result: std::result::Result<View, String> },
    Usage { link: u64, result: std::result::Result<Plans, String> },
}

impl From<Report> for AppEvent {
    fn from(report: Report) -> Self {
        Self::Machine(MachineEvent::Link(report))
    }
}

pub(super) struct Held {
    pub(super) name: String,
    pub(super) destination: String,
    pub(super) link: Link,
    pub(super) status: Status,
    pub(super) cache: Cache,
}

pub(super) struct Machines {
    held: Vec<Held>,
    connect: Connector,
    update: Updater,
    network: Network,
    waiting: Vec<(u64, u64)>,
    synced: Vec<Machine>,
    servers: Servers,
    next: u64,
    sleep: Sleep,
}

impl Machines {
    pub(super) fn new() -> Self {
        Self {
            held: Vec::new(),
            connect: Box::new(link::ssh),
            update: Arc::new(|destination| link::remote(destination).install_update()),
            network: Network::new(Box::new(network::default_route)),
            waiting: Vec::new(),
            synced: Vec::new(),
            servers: Servers::new(control::server_token()),
            next: 1,
            sleep: Sleep::default(),
        }
    }

    pub(super) fn held(&self) -> &[Held] {
        &self.held
    }

    pub(super) fn by_link(&mut self, link: u64) -> Option<&mut Held> {
        self.held.iter_mut().find(|held| held.link.id() == link)
    }

    pub(super) fn named(&mut self, name: &str) -> Option<&mut Held> {
        self.held.iter_mut().find(|held| held.name.eq_ignore_ascii_case(name))
    }

    pub(super) fn connector(&self, destination: &str) -> Box<dyn Connect> {
        (self.connect)(destination)
    }

    pub(super) fn reason(&self, held: &Held) -> Option<String> {
        if let Some(failed) = &held.status.update_failed {
            return Some(failed.clone());
        }
        let down = held.status.down.as_ref()?;
        let other = match down {
            Down::SameServer(other) => self.held.iter().find(|h| h.link.id() == *other).map(|h| h.name.as_str()),
            _ => None,
        };
        down.reason(&held.name, other)
    }

    pub(super) fn retry(&self, name: Option<&str>) {
        let picked = |held: &&Held| name.is_none_or(|name| held.name.eq_ignore_ascii_case(name));
        for held in self.held.iter().filter(picked) {
            log::info!("machines", "retrying", name = held.name);
            held.link.poke();
        }
    }

    pub(super) fn wait_for_update(&mut self, link: u64, key: u64) {
        self.waiting.push((link, key));
    }

    fn log_down(&self, held: &Held, failed: u32) {
        let why = self.reason(held).unwrap_or_else(|| OLDER.into());
        let error = machines::hide_destination(&why, &held.name, &held.destination);
        log::warning!("machines", "machine unreachable", name = held.name, failed = failed, error = error);
    }
}

#[cfg(test)]
impl Machines {
    pub(super) fn set_connector(&mut self, connect: Connector) {
        self.connect = connect;
    }

    pub(super) fn set_updater(&mut self, update: Updater) {
        self.update = update;
    }

    pub(super) fn set_network(&mut self, network: Network) {
        self.network = network;
    }

    pub(super) fn set_status(&mut self, at: usize, status: Status) {
        self.held[at].status = status;
    }
}

fn updated(result: Result<Result<()>>, name: &str) -> std::result::Result<(), String> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(link::failure(&e, name)),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Update {
    Started(u64),
    Running,
    NotOlder,
    Unknown,
}

impl App {
    pub(super) fn sync_machines(&mut self, now: Instant, clock: i64) {
        self.follow_machines();
        let woke = self.machines.sleep.woke(clock, now);
        let moved = if self.machines.held.is_empty() {
            self.machines.network.rest();
            false
        } else {
            self.machines.network.moved(now)
        };
        if (woke || moved) && !self.machines.held.is_empty() {
            log::info!("machines", "retrying every link", woke = woke, network = moved);
            self.machines.held.iter().for_each(|held| held.link.poke());
        }
        self.refresh_machines_page();
    }

    pub(super) fn follow_machines(&mut self) {
        if self.machines.synced != self.config.machines {
            self.match_machines();
        }
    }

    fn match_machines(&mut self) {
        let listed = self.config.machines.clone();
        let before = std::mem::replace(&mut self.machines.synced, listed.clone());
        let wanted: Vec<Machine> = listed
            .into_iter()
            .zip(machines::problems(&self.config.machines))
            .filter_map(|(machine, problem)| problem.is_none().then_some(machine))
            .collect();
        self.machines.held.retain(|held| {
            let kept = wanted.iter().any(|machine| machine.destination == held.destination);
            if !kept {
                log::info!("machines", "machine dropped", name = held.name, link = held.link.id());
            }
            kept
        });
        for machine in wanted {
            let edited = before.iter().any(|old| old.destination == machine.destination && *old != machine);
            let Some(held) = self.machines.held.iter_mut().find(|held| held.destination == machine.destination) else {
                self.start_machine_link(machine);
                continue;
            };
            held.name = machine.name;
            if edited && held.status.state != State::Linked {
                held.link.poke();
            }
        }
    }

    fn start_machine_link(&mut self, machine: Machine) {
        let id = self.machines.next;
        self.machines.next += 1;
        let connector = (self.machines.connect)(&machine.destination);
        let link = Link::start(id, connector, &self.machines.servers, &self.tx);
        log::info!("machines", "linking", name = machine.name, link = id);
        let (name, destination) = (machine.name, machine.destination);
        let held = Held { name, destination, link, status: Status::default(), cache: Cache::default() };
        self.machines.held.push(held);
    }

    pub(super) fn machine_event(&mut self, event: MachineEvent) {
        match event {
            MachineEvent::Link(report) => self.machine_reported(report),
            MachineEvent::Updated { link, result, hidden } => self.machine_updated(link, &result, &hidden),
            MachineEvent::Stats { link, result } => self.machine_answered(link, result),
            MachineEvent::Usage { link, result } => self.machine_answered(link, result),
        }
    }

    fn machine_reported(&mut self, report: Report) {
        let Some(at) = self.machines.held.iter().position(|held| held.link.id() == report.link) else { return };
        let now = crate::time::now();
        let held = &mut self.machines.held[at];
        match report.event {
            Event::Linked { release, server_version } => {
                if held.status.state != State::Linked {
                    log::info!("machines", "machine linked", name = held.name, version = release);
                }
                held.status.linked(release, server_version, now);
            }
            Event::Down { down, failed } => {
                held.status.down(down, now);
                if !held.another_computer() {
                    held.cache = Cache::default();
                }
                if failed.is_power_of_two() {
                    self.machines.log_down(&self.machines.held[at], failed);
                }
            }
        }
    }

    pub(super) fn update_machine(&mut self, name: &str) -> Update {
        let Some(held) = self.machines.named(name) else { return Update::Unknown };
        if held.status.updating {
            return Update::Running;
        }
        if held.status.state != State::Older {
            return Update::NotOlder;
        }
        held.status.updating = true;
        held.status.update_failed = None;
        let (link, destination, name) = (held.link.id(), held.destination.clone(), held.name.clone());
        let update = Arc::clone(&self.machines.update);
        let job = Job::new(Level::Info, "machines", "update").with("name", &name);
        let there = destination.clone();
        let work = move || Ok(update(&there));
        self.spawn_job(job, work, move |result, job| {
            let result = updated(result, &name);
            let hidden = result.clone().map_err(|why| machines::hide_destination(&why, &name, &destination));
            if let Err(why) = &hidden {
                job.fail(why);
            }
            Some(AppEvent::Machine(MachineEvent::Updated { link, result, hidden }))
        });
        Update::Started(link)
    }

    fn machine_updated(
        &mut self,
        link: u64,
        result: &std::result::Result<(), String>,
        told: &std::result::Result<(), String>,
    ) {
        if let Some(held) = self.machines.by_link(link) {
            held.status.updating = false;
            match result {
                Ok(()) => held.link.poke(),
                Err(why) => held.status.update_failed = Some(why.clone()),
            }
        }
        let (asked, left): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.machines.waiting).into_iter().partition(|(waits, _)| *waits == link);
        self.machines.waiting = left;
        for (_, key) in asked {
            let result = told.clone();
            self.answer_read(key, move |app| match result {
                Ok(()) => Ok(json(&app.machine_list())),
                Err(why) => Err(Refusal::new(Code::Failed, why)),
            });
        }
    }

    pub(super) fn machine_infos(&self) -> Vec<Info> {
        let problems = machines::problems(&self.config.machines);
        let rows = self.config.machines.iter().zip(problems);
        rows.map(|(machine, problem)| {
            let unheld = problem.is_some();
            let held = self.machines.held.iter().find(|held| !unheld && held.destination == machine.destination);
            let error = held.and_then(|held| self.machines.reason(held));
            Info::of(machine, held.map(|held| &held.status), problem, error)
        })
        .collect()
    }

    pub(super) fn refresh_machines_page(&mut self) {
        if !matches!(&self.overlay, Some(Overlay::Settings(s)) if s.page == Page::Machines) {
            return;
        }
        let infos = self.machine_infos();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.take_machines(infos);
            s.now = SystemTime::now();
        }
    }

    pub(super) fn stop_machines(&mut self) {
        self.machines.held.clear();
        self.machines.synced.clear();
    }
}
