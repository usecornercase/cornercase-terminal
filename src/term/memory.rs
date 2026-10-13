use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use crate::process;

pub const MEASURE_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct Pane {
    bytes: Option<u64>,
    measured: Option<Instant>,
    measuring: Option<Receiver<Option<u64>>>,
}

impl Pane {
    pub fn update(&mut self, pid: i32, now: Instant) {
        match self.measuring.as_ref().map(Receiver::try_recv) {
            Some(Ok(bytes)) => {
                self.measuring = None;
                self.bytes = bytes;
            }
            Some(Err(TryRecvError::Disconnected)) => self.measuring = None,
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        if self.measuring.is_none() && self.measured.is_none_or(|at| now.duration_since(at) >= MEASURE_EVERY) {
            self.measured = Some(now);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || tx.send(measure(pid)));
            self.measuring = Some(rx);
        }
    }

    pub fn bytes(&self) -> Option<u64> {
        self.bytes
    }
}

fn measure(pid: i32) -> Option<u64> {
    std::iter::once(pid).chain(process::descendants(pid)).filter_map(process::footprint).reduce(u64::saturating_add)
}

#[cfg(test)]
mod tests;
