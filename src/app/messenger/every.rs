use std::time::{Duration, Instant};

use crate::log;

const EVERY: Duration = Duration::from_secs(3);

#[derive(Debug, Default)]
pub(super) struct Every {
    asked: Option<Instant>,
    until: Option<Instant>,
    busy: bool,
    pub(super) failed: Option<String>,
}

impl Every {
    pub(super) fn closed(&mut self) {
        self.asked = None;
        self.until = None;
        self.failed = None;
    }

    pub(super) fn idle(&self) -> bool {
        !self.busy
    }

    pub(super) fn wait(&self, now: Instant) -> Duration {
        self.wait_every(now, EVERY)
    }

    pub(super) fn wait_every(&self, now: Instant, every: Duration) -> Duration {
        let every = self.asked.map_or(Duration::ZERO, |at| every.saturating_sub(now.duration_since(at)));
        every.max(self.until.map_or(Duration::ZERO, |until| until.saturating_duration_since(now)))
    }

    pub(super) fn start(&mut self, now: Instant) {
        self.asked = Some(now);
        self.busy = true;
    }

    pub(super) fn landed(&mut self) {
        self.busy = false;
    }

    pub(super) fn later(&mut self, wait: Duration) {
        self.failed = None;
        self.until = (!wait.is_zero()).then(|| Instant::now() + wait);
    }

    pub(super) fn fail(&mut self, target: &'static str, what: &'static str, why: String) {
        if self.failed.as_ref() != Some(&why) {
            log::warning!(target, what, error = why);
        }
        self.failed = Some(why);
    }
}
