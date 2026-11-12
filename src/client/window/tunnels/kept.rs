use std::time::Instant;

use super::{CHECK_EVERY, Held, Tunnels};
use crate::protocol::TunnelState;
use crate::remote::tunnel::{Forward, Owner};
use crate::remote::tunnels::{self, Kept};

impl Tunnels {
    pub(super) fn adopt_kept(&mut self) -> Vec<u16> {
        let Some(destination) = self.destination.clone() else { return Vec::new() };
        let kept = tunnels::read(&self.file, &destination);
        kept.iter().filter(|kept| self.adopt(kept)).map(|kept| kept.port).collect()
    }

    fn adopt(&mut self, kept: &Kept) -> bool {
        let Some(held) = self.held.get_mut(&kept.port) else {
            let mut held = Held::fresh();
            held.local = kept.local;
            if kept.closed {
                held.state = TunnelState::Closed;
            }
            self.held.insert(kept.port, held);
            return !kept.closed;
        };
        held.local = held.local.or(kept.local);
        match (kept.closed, held.closed()) {
            (true, false) => {
                held.stop();
                false
            }
            (false, true) => {
                held.failures = 0;
                held.was_up = false;
                true
            }
            _ => false,
        }
    }

    fn entry(&self, port: u16) -> Option<Kept> {
        let destination = self.destination.as_deref()?;
        tunnels::read(&self.file, destination).into_iter().find(|kept| kept.port == port)
    }

    fn owns(&self, pid: i32) -> bool {
        self.held.values().any(|held| held.started.as_ref().and_then(|started| started.pid) == Some(pid))
    }

    pub(super) fn claimed_elsewhere(&mut self, port: u16, now: Instant) -> bool {
        let Some(destination) = self.destination.clone() else { return false };
        let Some(Kept { pid: Some(pid), local: Some(local), closed: false, .. }) = self.entry(port) else {
            return false;
        };
        if self.owns(pid) {
            return false;
        }
        let state = match self.spawner.owner(pid, &destination, Forward { port, local }) {
            Owner::Gone => return false,
            Owner::Listening => TunnelState::Up,
            Owner::Starting => TunnelState::Opening,
        };
        self.follow(port, (pid, local), state, now);
        true
    }

    fn follow(&mut self, port: u16, (pid, local): (i32, u16), state: TunnelState, now: Instant) {
        let held = self.held.entry(port).or_insert_with(Held::fresh);
        held.elsewhere = Some(pid);
        held.local = Some(local);
        held.started = None;
        held.retry_at = None;
        held.settling = None;
        if state == TunnelState::Up {
            held.was_up = true;
            held.failures = 0;
        }
        held.state = state;
        held.check_at = Some(now + CHECK_EVERY);
    }

    pub(super) fn converge(&mut self, port: u16, now: Instant) {
        let Some(destination) = self.destination.clone() else { return };
        let Some(held) = self.held.get(&port) else { return };
        let (Some(own), Some(local)) = (held.started.as_ref().and_then(|started| started.pid), held.local) else {
            return;
        };
        if let Some(Kept { pid: Some(other), local: Some(other_local), closed: false, .. }) = self.entry(port)
            && other != own
            && self.spawner.owner(other, &destination, Forward { port, local: other_local }) == Owner::Listening
        {
            self.follow(port, (other, other_local), TunnelState::Up, now);
        } else {
            self.claim(port, local, Some(own));
        }
    }

    pub(super) fn settle_later(&mut self, port: u16, own: Option<i32>, reason: &str, now: Instant) -> bool {
        if own.is_none() || self.entry(port).is_none_or(|kept| kept.pid != own) {
            return false;
        }
        let Some(held) = self.held.get_mut(&port) else { return false };
        held.state = TunnelState::Opening;
        held.settling = Some(super::one_line(reason));
        held.check_at = Some(now + CHECK_EVERY);
        true
    }

    pub(super) fn check_elsewhere(&mut self, now: Instant) -> bool {
        let due: Vec<u16> = self
            .held
            .iter()
            .filter(|(_, held)| held.check_at.is_some_and(|at| at <= now))
            .map(|(port, _)| *port)
            .collect();
        let mut changed = false;
        for port in due {
            changed |= self.check(port, now);
        }
        changed
    }

    fn settle(&mut self, port: u16, now: Instant) -> bool {
        let Some(reason) = self.held.get_mut(&port).and_then(|held| held.settling.take()) else { return false };
        if let Some(held) = self.held.get_mut(&port) {
            held.check_at = None;
        }
        if !self.claimed_elsewhere(port, now)
            && let Some(held) = self.held.get_mut(&port)
        {
            held.state = TunnelState::Down(reason);
        }
        true
    }

    fn check(&mut self, port: u16, now: Instant) -> bool {
        if self.held.get(&port).is_some_and(|held| held.settling.is_some()) {
            return self.settle(port, now);
        }
        let Some(destination) = self.destination.clone() else { return false };
        let Some(held) = self.held.get_mut(&port) else { return false };
        let (Some(pid), Some(local)) = (held.elsewhere, held.local) else { return false };
        let state = match self.spawner.owner(pid, &destination, Forward { port, local }) {
            Owner::Gone => {
                self.take_over(port, now);
                return true;
            }
            Owner::Listening => TunnelState::Up,
            Owner::Starting => TunnelState::Opening,
        };
        let Some(held) = self.held.get_mut(&port) else { return false };
        held.check_at = Some(now + CHECK_EVERY);
        if state == TunnelState::Up {
            held.was_up = true;
            held.failures = 0;
        }
        std::mem::replace(&mut held.state, state.clone()) != state
    }

    fn take_over(&mut self, port: u16, now: Instant) {
        if let Some(held) = self.held.get_mut(&port) {
            held.elsewhere = None;
            held.check_at = None;
        }
        match self.entry(port) {
            None => {
                self.held.remove(&port);
            }
            Some(Kept { closed: true, .. }) => {
                if let Some(held) = self.held.get_mut(&port) {
                    held.stop();
                }
            }
            Some(_) => self.start(port, now),
        }
    }

    pub(super) fn claim(&self, port: u16, local: u16, pid: Option<i32>) {
        let kept = Kept { port, local: Some(local), pid, closed: false };
        self.write(port, |_| Some(kept));
    }

    pub(super) fn remember_open(&self, port: u16) {
        self.write(port, |old| Some(Kept { closed: false, ..old.unwrap_or(Kept::new(port)) }));
    }

    pub(super) fn remember_closed(&self, port: u16, local: Option<u16>) {
        self.write(port, |_| Some(Kept { port, local, pid: None, closed: true }));
    }

    pub(super) fn forget_kept(&self, port: u16) {
        if let Some(destination) = self.destination.as_deref() {
            let _ = tunnels::forget(&self.file, destination, port);
        }
    }

    fn write(&self, port: u16, change: impl FnOnce(Option<Kept>) -> Option<Kept>) {
        let Some(destination) = self.destination.as_deref() else { return };
        let _ = tunnels::update(&self.file, destination, port, change);
    }
}
