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
    std::iter::once(pid).chain(process::descendants(pid)).filter_map(process::resident).reduce(u64::saturating_add)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{Family, exited_pid, this_pid, wait_until};

    fn measured(pid: i32, now: Instant) -> Pane {
        let mut pane = Pane::default();
        wait_until("the memory is measured", || {
            pane.update(pid, now);
            pane.bytes().is_some()
        });
        pane
    }

    #[test]
    fn counts_the_process_and_everything_it_started() {
        let family = Family::new();
        let parent =
            [family.pid(), family.child().expect("a child")].map(|pid| process::resident(pid).expect("resident"));

        let pane = measured(family.pid(), Instant::now());

        assert!(pane.bytes() > Some(parent.iter().sum()), "{:?} <= {parent:?}", pane.bytes());
    }

    #[test]
    fn measures_again_once_two_seconds_have_passed() {
        let pid = this_pid();
        let now = Instant::now();
        let mut pane = measured(pid, now);

        pane.update(pid, now + MEASURE_EVERY.saturating_sub(Duration::from_millis(1)));
        let early = pane.measuring.is_some();
        pane.update(pid, now + MEASURE_EVERY);

        assert_eq!((early, pane.measuring.is_some()), (false, true));
    }

    #[test]
    fn a_process_that_exited_has_no_memory() {
        let (pid, mut pane) = (exited_pid(), Pane::default());

        wait_until("the measure ends", || {
            pane.update(pid, Instant::now());
            pane.measuring.is_none()
        });

        assert_eq!(pane.bytes(), None);
    }
}
