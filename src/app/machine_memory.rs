use std::time::{Duration, SystemTime};

use super::App;
use super::machine_links::Held;
use crate::host::memory::{self as host_memory, Reader};
use crate::machines::State;
use crate::machines::memory::MemoryInfo;
use crate::time::Stamp;

pub(super) const REFRESH: Duration = Duration::from_secs(30);

pub(super) struct MemoryState {
    pub(super) read: Reader,
}

impl Default for MemoryState {
    fn default() -> Self {
        Self { read: host_memory::read }
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
}
