use std::time::{Duration, Instant};

use crate::host::network::Route;

const SLEPT: Duration = Duration::from_secs(5);
const READ_EVERY: Duration = Duration::from_secs(2);

pub(crate) fn slept(wall_before: i64, before: Instant, wall_now: i64, now: Instant) -> bool {
    let wall = Duration::from_secs(u64::try_from(wall_now.saturating_sub(wall_before)).unwrap_or(0));
    wall.saturating_sub(now.saturating_duration_since(before)) >= SLEPT
}

#[derive(Debug, Default)]
pub(crate) struct Sleep {
    last: Option<(i64, Instant)>,
}

impl Sleep {
    pub(crate) fn woke(&mut self, clock: i64, now: Instant) -> bool {
        let woke = self.last.is_some_and(|(wall, before)| slept(wall, before, clock, now));
        self.last = Some((clock, now));
        woke
    }
}

pub(crate) fn changed(before: Option<&Route>, now: &Route) -> bool {
    before.is_some_and(|before| before != now)
}

pub(crate) struct Network {
    probe: Box<dyn FnMut() -> Route + Send>,
    last: Option<(Instant, Route)>,
}

impl Network {
    pub(crate) fn new(probe: Box<dyn FnMut() -> Route + Send>) -> Self {
        Self { probe, last: None }
    }

    pub(crate) fn moved(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|(at, _)| now.saturating_duration_since(at) < READ_EVERY) {
            return false;
        }
        let route = (self.probe)();
        let moved = changed(self.last.as_ref().map(|(_, before)| before), &route);
        self.last = Some((now, route));
        moved
    }

    pub(crate) fn rest(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests;
