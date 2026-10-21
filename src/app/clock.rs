use std::time::{Duration, Instant};

use super::worktrees::SYNC_EVERY;

pub(super) struct Clock {
    booted: Instant,
    synced: Option<Instant>,
}

impl Clock {
    pub(super) fn new() -> Self {
        Self { booted: Instant::now(), synced: None }
    }

    pub(super) fn since_boot(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.booted)
    }

    pub(super) fn sync_due(&mut self, now: Instant) -> bool {
        if self.synced.is_some_and(|at| now.duration_since(at) < SYNC_EVERY) {
            return false;
        }
        self.synced = Some(now);
        true
    }
}
