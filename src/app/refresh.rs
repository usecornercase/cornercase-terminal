use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(super) struct Refresh {
    generation: u64,
    busy: bool,
    asked: Option<Instant>,
}

impl Refresh {
    pub(super) fn open(&mut self) -> u64 {
        self.generation += 1;
        self.busy = false;
        self.asked = None;
        self.generation
    }

    pub(super) fn due(&self, now: Instant, every: Duration) -> bool {
        !self.busy && self.asked.is_none_or(|at| now.saturating_duration_since(at) >= every)
    }

    pub(super) fn begin(&mut self, now: Instant) {
        self.busy = true;
        self.asked = Some(now);
    }

    pub(super) fn answered(&mut self, generation: u64) -> bool {
        let current = generation == self.generation;
        if current {
            self.busy = false;
        }
        current
    }

    pub(super) fn again(&mut self) {
        self.asked = None;
    }

    #[cfg(test)]
    pub(super) fn busy(&self) -> bool {
        self.busy
    }
}
