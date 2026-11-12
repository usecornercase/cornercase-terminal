use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::time::Instant;

use super::Incoming;
use crate::host::open::{Browse, Opener, Openers};
use crate::protocol::{Holding, Tunnel, TunnelOrder, TunnelState};
use crate::remote::tunnel::{self, Forward, Running, Watched};
use crate::remote::{self};
use crate::text;

const MAX_TUNNELS: usize = 32;
const REASON_CHARS: usize = 300;
const TOO_MANY: &str = "a window holds at most 32 tunnels";

#[derive(Default)]
pub(super) struct Started {
    _running: Option<Running>,
}

pub(super) trait Spawn {
    fn spawn(&mut self, destination: &str, forward: Forward, attempt: u64) -> Result<Started, String>;
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
            .map(|running| Started { _running: Some(running) })
            .map_err(|error| remote::reason(&error))
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
        }
    }

    fn down(&self) -> bool {
        matches!(self.state, TunnelState::Down(_))
    }
}

pub(super) struct Tunnels {
    spawner: Box<dyn Spawn>,
    browser: Openers,
    destination: Option<String>,
    held: BTreeMap<u16, Held>,
    attempts: u64,
    applied: u64,
}

fn one_line(reason: &str) -> String {
    text::one_line(reason).chars().take(REASON_CHARS).collect()
}

impl Tunnels {
    pub(super) fn new(spawner: Box<dyn Spawn>, browser: Openers) -> Self {
        Self { spawner, browser, destination: None, held: BTreeMap::new(), attempts: 0, applied: 0 }
    }

    pub(super) fn sync(&mut self, destination: Option<&str>) {
        if self.destination.as_deref() != destination {
            self.held.clear();
            self.destination = destination.map(str::to_string);
            return;
        }
        let down: Vec<u16> = self.held.iter().filter(|(_, held)| held.down()).map(|(port, _)| *port).collect();
        for port in down {
            self.start(port);
        }
    }

    pub(super) fn order(&mut self, order: TunnelOrder) {
        self.applied += 1;
        match order {
            TunnelOrder::Open(0) | TunnelOrder::Close(0) => {}
            TunnelOrder::Open(port) => self.open(port),
            TunnelOrder::Close(port) => {
                self.held.remove(&port);
            }
        }
    }

    fn open(&mut self, port: u16) {
        match self.held.get(&port).map(|held| &held.state) {
            Some(TunnelState::Opening | TunnelState::Up) => return,
            None if self.held.len() >= MAX_TUNNELS * 2 => return,
            _ => {}
        }
        if let Some(held) = self.held.get_mut(&port) {
            held.failures = 0;
            held.was_up = false;
        }
        self.start(port);
    }

    fn start(&mut self, port: u16) {
        let Some(destination) = self.destination.clone() else { return };
        if self.running_besides(port) >= MAX_TUNNELS {
            let held = self.held.entry(port).or_insert_with(Held::fresh);
            held.retry_at = None;
            held.started = None;
            held.state = TunnelState::Down(TOO_MANY.into());
            return;
        }
        self.attempts += 1;
        let attempt = self.attempts;
        let before = self.held.get(&port).and_then(|held| held.local);
        let launcher = &mut self.spawner;
        let begun = tunnel::local_port(port, before).map_err(|error| error.to_string()).and_then(|local| {
            launcher.spawn(&destination, Forward { port, local }, attempt).map(|started| (local, started))
        });
        let held = self.held.entry(port).or_insert_with(Held::fresh);
        held.retry_at = None;
        held.attempt = attempt;
        match begun {
            Ok((local, started)) => {
                held.local = Some(local);
                held.state = TunnelState::Opening;
                held.started = Some(started);
            }
            Err(reason) => {
                held.state = TunnelState::Down(one_line(&reason));
                held.started = None;
            }
        }
    }

    fn running_besides(&self, port: u16) -> usize {
        self.held.iter().filter(|(other, held)| **other != port && held.started.is_some()).count()
    }

    pub(super) fn watched(&mut self, attempt: u64, watched: Watched, now: Instant) -> bool {
        let Some(held) = self.held.values_mut().find(|held| held.attempt == attempt) else { return false };
        match watched {
            Watched::Up => {
                held.state = TunnelState::Up;
                held.failures = 0;
                held.was_up = true;
            }
            Watched::Ended(reason) => {
                if held.was_up {
                    held.failures += 1;
                    held.retry_at = Some(now + remote::backoff(held.failures));
                }
                held.state = TunnelState::Down(one_line(&reason));
                held.started = None;
            }
        }
        true
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
            self.start(*port);
        }
        !due.is_empty()
    }

    pub(super) fn next_wake(&self, linked: bool) -> Option<Instant> {
        if !linked {
            return None;
        }
        self.held.values().filter(|held| held.down()).filter_map(|held| held.retry_at).min()
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
