use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::control::auto::AutoWait;
use super::machine_links::Held;
use crate::host::memory::{self as host_memory, Reader};
use crate::machines::State;
use crate::machines::memory::{self, Candidate, Left, MemoryInfo, Picked, Reservations};
use crate::time::Stamp;

pub(super) const REFRESH: Duration = Duration::from_secs(30);

pub(super) struct MemoryState {
    pub(super) read: Reader,
    pub(super) reservations: Reservations,
    pub(super) waiting: Vec<AutoWait>,
}

impl Default for MemoryState {
    fn default() -> Self {
        Self { read: host_memory::read, reservations: Reservations::default(), waiting: Vec::new() }
    }
}

fn age(at: SystemTime) -> Duration {
    SystemTime::now().duration_since(at).unwrap_or_default()
}

impl App {
    pub(super) fn here_memory(&self) -> Option<MemoryInfo> {
        let read = (self.machines.memory.read)().ok()?;
        Some(MemoryInfo::of(read, Some(Stamp(SystemTime::now()).to_string())))
    }

    pub(super) fn held_memory(held: &Held) -> Option<MemoryInfo> {
        let (info, at) = held.cache.memory.read.as_ref().filter(|_| held.another_computer())?;
        Some(MemoryInfo { read_at: Some(Stamp(*at).to_string()), ..info.clone() })
    }

    pub(super) fn memory_answered(&mut self, link: u64, result: Result<MemoryInfo, String>) {
        self.machine_answered::<MemoryInfo>(link, result);
        self.auto_waiting(Instant::now());
    }

    fn memory_stale(held: &Held, older_than: Duration) -> bool {
        let cached = &held.cache.memory;
        !cached.asking && cached.read.as_ref().is_none_or(|(_, at)| age(*at) >= older_than)
    }

    pub(super) fn refresh_memory(&mut self, older_than: Duration) -> Vec<u64> {
        let stale: Vec<String> = self
            .machines_in_order()
            .filter(|held| held.status.state == State::Linked && Self::memory_stale(held, older_than))
            .map(|held| held.name.clone())
            .collect();
        stale.iter().filter_map(|name| self.ask_machine::<MemoryInfo>(name)).collect()
    }

    pub(super) fn memory_asking(&self) -> bool {
        self.machines_in_order().any(|held| held.cache.memory.asking)
    }

    fn candidate(&self, held: &Held, project: u64, now: Instant) -> Candidate {
        let read = held.cache.memory.read.as_ref().filter(|_| held.another_computer());
        let left = match held.status.state {
            State::Linked if held.seen.is_none() => Some(Left::Connecting),
            State::Linked if self.project_there(held.link.id(), project).is_none() => Some(Left::NoProject),
            State::Linked => None,
            State::Connecting => Some(Left::Connecting),
            State::Unreachable | State::Older => Some(Left::Unreachable),
        };
        Candidate {
            name: Some(held.name.clone()),
            memory: read.and_then(|(info, _)| info.memory()),
            age: read.map(|(_, at)| age(*at)),
            left,
            reserved: self.machines.memory.reservations.on(Some(&held.name), now),
        }
    }

    pub(super) fn memory_candidates(&self, project: u64, now: Instant) -> Vec<Candidate> {
        let here = (self.machines.memory.read)().ok();
        let mut candidates = vec![Candidate::here(here, self.machines.memory.reservations.on(None, now))];
        candidates.extend(self.machines_in_order().map(|held| self.candidate(held, project, now)));
        candidates
    }

    pub(super) fn pick_machine(&self, project: u64, needs: u64, now: Instant) -> Picked {
        memory::pick(needs, &self.memory_candidates(project, now))
    }

    pub(super) fn reserve_memory(&mut self, target: Option<&str>, needs: u64, now: Instant) {
        self.machines.memory.reservations.reserve(target, needs, now);
    }
}
