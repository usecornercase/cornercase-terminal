use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::Incoming;
use crate::host::open::{Browse, Opener, Openers};
use crate::protocol::{Holding, Tunnel, TunnelOrder, TunnelState};
use crate::remote::tunnel::{self, Forward, Owner, Running, Watched};
use crate::remote::{self};

mod kept;

const MAX_TUNNELS: usize = 32;
const CHECK_EVERY: Duration = Duration::from_secs(1);
const TOO_MANY: &str = "a window holds at most 32 tunnels";

#[derive(Default)]
pub(super) struct Started {
    _running: Option<Running>,
    pid: Option<i32>,
}

pub(super) trait Spawn {
    fn spawn(&mut self, destination: &str, forward: Forward, attempt: u64) -> Result<Started, String>;
    fn owner(&self, pid: i32, destination: &str, forward: Forward) -> Owner;
}

pub(super) struct Ssh {
    pub(super) tx: Sender<Incoming>,
}

impl Spawn for Ssh {
    fn spawn(&mut self, destination: &str, forward: Forward, attempt: u64) -> Result<Started, String> {
        let tx = self.tx.clone();
        let report = move |watched| {
            let _ = tx.send(Incoming::Tunnel(attempt, watched));
        };
        tunnel::spawn(destination, forward, report)
            .map(|running| Started { pid: Some(running.pid()), _running: Some(running) })
            .map_err(|error| remote::reason(&error))
    }

    fn owner(&self, pid: i32, destination: &str, forward: Forward) -> Owner {
        tunnel::owner(pid, destination, forward)
    }
}

struct Held {
    local: Option<u16>,
    state: TunnelState,
    started: Option<Started>,
    attempt: u64,
    failures: u32,
    retry_at: Option<Instant>,
    was_up: bool,
    elsewhere: Option<i32>,
    check_at: Option<Instant>,
    settling: Option<String>,
}

impl Held {
    fn fresh() -> Self {
        Self {
            local: None,
            state: TunnelState::Opening,
            started: None,
            attempt: 0,
            failures: 0,
            retry_at: None,
            was_up: false,
            elsewhere: None,
            check_at: None,
            settling: None,
        }
    }

    fn stop(&mut self) {
        self.started = None;
        self.attempt = 0;
        self.elsewhere = None;
        self.retry_at = None;
        self.check_at = None;
        self.settling = None;
        self.state = TunnelState::Closed;
    }

    fn closed(&self) -> bool {
        self.state == TunnelState::Closed
    }

    fn down(&self) -> bool {
        matches!(self.state, TunnelState::Down(_))
    }
}

pub(super) struct Tunnels {
    spawner: Box<dyn Spawn>,
    browser: Openers,
    file: PathBuf,
    destination: Option<String>,
    held: BTreeMap<u16, Held>,
    attempts: u64,
    applied: u64,
}

impl Tunnels {
    pub(super) fn new(spawner: Box<dyn Spawn>, browser: Openers, file: PathBuf) -> Self {
        Self { spawner, browser, file, destination: None, held: BTreeMap::new(), attempts: 0, applied: 0 }
    }

    pub(super) fn sync(&mut self, destination: Option<&str>, now: Instant) {
        if self.destination.as_deref() != destination {
            self.held.clear();
            self.destination = destination.map(str::to_string);
        }
        let mut begin = self.adopt_kept();
        for (port, held) in &self.held {
            if held.down() && !begin.contains(port) {
                begin.push(*port);
            }
        }
        for port in begin {
            self.start(port, now);
        }
    }

    pub(super) fn order(&mut self, order: TunnelOrder, now: Instant) {
        self.applied += 1;
        match order {
            TunnelOrder::Open(0) | TunnelOrder::Close(0) | TunnelOrder::Forget(0) => {}
            TunnelOrder::Open(port) => self.open(port, now),
            TunnelOrder::Close(port) => self.close(port),
            TunnelOrder::Forget(port) => self.forget(port),
        }
    }

    fn open(&mut self, port: u16, now: Instant) {
        match self.held.get(&port).map(|held| &held.state) {
            Some(TunnelState::Opening | TunnelState::Up) => return,
            None if self.held.len() >= MAX_TUNNELS * 2 => return,
            _ => {}
        }
        if self.running_besides(port) < MAX_TUNNELS {
            self.remember_open(port);
        }
        if let Some(held) = self.held.get_mut(&port) {
            held.failures = 0;
            held.was_up = false;
        }
        self.start(port, now);
    }

    fn close(&mut self, port: u16) {
        let Some(held) = self.held.get_mut(&port) else { return };
        held.stop();
        let local = held.local;
        self.remember_closed(port, local);
    }

    fn forget(&mut self, port: u16) {
        self.held.remove(&port);
        self.forget_kept(port);
    }

    fn start(&mut self, port: u16, now: Instant) {
        let Some(destination) = self.destination.clone() else { return };
        if self.claimed_elsewhere(port, now) {
            return;
        }
        if self.running_besides(port) >= MAX_TUNNELS {
            let held = self.held.entry(port).or_insert_with(Held::fresh);
            held.retry_at = None;
            held.started = None;
            held.state = TunnelState::Down(TOO_MANY.into());
            return;
        }
        self.attempts += 1;
        let attempt = self.attempts;
        let begun = self.begin(&destination, port, attempt);
        let held = self.held.entry(port).or_insert_with(Held::fresh);
        held.retry_at = None;
        held.attempt = attempt;
        held.elsewhere = None;
        held.check_at = None;
        held.settling = None;
        match begun {
            Ok((local, started)) => {
                held.local = Some(local);
                held.state = TunnelState::Opening;
                let pid = started.pid;
                held.started = Some(started);
                self.claim(port, local, pid);
            }
            Err(reason) => {
                held.state = TunnelState::Down(tunnel::reason(&reason));
                held.started = None;
            }
        }
    }

    fn begin(&mut self, destination: &str, port: u16, attempt: u64) -> Result<(u16, Started), String> {
        let before = self.held.get(&port).and_then(|held| held.local);
        let launcher = &mut self.spawner;
        tunnel::local_port(port, before).map_err(|error| error.to_string()).and_then(|local| {
            launcher.spawn(destination, Forward { port, local }, attempt).map(|started| (local, started))
        })
    }

    fn running_besides(&self, port: u16) -> usize {
        self.held.iter().filter(|(other, held)| **other != port && held.started.is_some()).count()
    }

    pub(super) fn watched(&mut self, attempt: u64, watched: Watched, now: Instant) -> bool {
        let Some(port) = self.held.iter().find(|(_, held)| held.attempt == attempt).map(|(port, _)| *port) else {
            return false;
        };
        match watched {
            Watched::Up => {
                if let Some(held) = self.held.get_mut(&port) {
                    held.state = TunnelState::Up;
                    held.failures = 0;
                    held.was_up = true;
                }
                self.converge(port, now);
            }
            Watched::Ended(reason) => self.ended(port, &reason, now),
        }
        true
    }

    fn ended(&mut self, port: u16, reason: &str, now: Instant) {
        let Some(held) = self.held.get_mut(&port) else { return };
        let own = held.started.take().and_then(|started| started.pid);
        if !held.was_up && (self.claimed_elsewhere(port, now) || self.settle_later(port, own, reason, now)) {
            return;
        }
        let Some(held) = self.held.get_mut(&port) else { return };
        if held.was_up {
            held.failures += 1;
            held.retry_at = Some(now + remote::backoff(held.failures));
        }
        held.state = TunnelState::Down(tunnel::reason(reason));
    }

    pub(super) fn tick(&mut self, now: Instant, linked: bool) -> bool {
        if !linked {
            return false;
        }
        let due: Vec<u16> = self
            .held
            .iter()
            .filter(|(_, held)| held.down() && held.retry_at.is_some_and(|at| at <= now))
            .map(|(port, _)| *port)
            .collect();
        for port in &due {
            self.start(*port, now);
        }
        let checked = self.check_elsewhere(now);
        !due.is_empty() || checked
    }

    pub(super) fn next_wake(&self, linked: bool) -> Option<Instant> {
        if !linked {
            return None;
        }
        let retries = self.held.values().filter(|held| held.down()).filter_map(|held| held.retry_at);
        let checks = self.held.values().filter_map(|held| held.check_at);
        retries.chain(checks).min()
    }

    pub(super) fn holding(&self) -> Holding {
        let tunnels = self
            .held
            .iter()
            .map(|(port, held)| Tunnel { port: *port, local: held.local, state: held.state.clone() })
            .collect();
        Holding { browse: self.browser.ids().contains(&Opener::Default), orders: self.applied, tunnels }
    }

    pub(super) fn browse(&self, browse: Browse) -> Result<Browse, String> {
        let local = match self.held.get(&browse.port) {
            Some(Held { state: TunnelState::Up, local: Some(local), .. })
                if browse.ip.is_none_or(|ip| ip.is_loopback()) =>
            {
                Some(*local)
            }
            _ => None,
        };
        local
            .map(|port| Browse { ip: None, port })
            .ok_or_else(|| format!("port {} is not tunnelled to this computer", browse.port))
    }

    pub(super) fn browser(&self) -> &Openers {
        &self.browser
    }
}

#[cfg(test)]
mod tests;
