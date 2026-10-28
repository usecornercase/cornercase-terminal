use std::thread;
use std::time::{Duration, Instant};

use rustix::process::Signal;

const GRACE: Duration = Duration::from_secs(3);

const KILL_WAIT: Duration = Duration::from_secs(1);

const POLL: Duration = Duration::from_millis(50);

pub(crate) trait Stoppable {
    fn check(&self) -> Result<(), String>;
    fn signal(&self, signal: Signal) -> Result<(), String>;
    fn holds(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Stopped,
    Killed,
    StillRunning,
}

impl Outcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Killed => "killed",
            Self::StillRunning => "still-running",
        }
    }
}

pub(crate) type Stopped<T> = (T, Result<Outcome, String>);

pub(crate) fn escalate<T: Stoppable>(targets: Vec<T>) -> Vec<Stopped<T>> {
    let mut done = Vec::new();
    let mut signalled = Vec::new();
    for target in targets {
        match target.check().and_then(|()| target.signal(Signal::TERM)) {
            Ok(()) => signalled.push(target),
            Err(reason) => done.push((target, Err(reason))),
        }
    }
    let left = wait(signalled, GRACE, Outcome::Stopped, &mut done);
    let killed = kill(left, &mut done);
    let left = wait(killed, KILL_WAIT, Outcome::Killed, &mut done);
    done.extend(left.into_iter().map(|target| (target, Ok(Outcome::StillRunning))));
    done
}

pub(crate) fn kill<T: Stoppable>(targets: Vec<T>, done: &mut Vec<Stopped<T>>) -> Vec<T> {
    let mut killed = Vec::new();
    for target in targets {
        if !target.holds() {
            done.push((target, Ok(Outcome::Stopped)));
            continue;
        }
        match target.signal(Signal::KILL) {
            Ok(()) => killed.push(target),
            Err(reason) => done.push((target, Err(reason))),
        }
    }
    killed
}

fn wait<T: Stoppable>(mut targets: Vec<T>, within: Duration, outcome: Outcome, done: &mut Vec<Stopped<T>>) -> Vec<T> {
    let until = Instant::now() + within;
    loop {
        let (released, holding): (Vec<T>, Vec<T>) = targets.into_iter().partition(|t| !t.holds());
        done.extend(released.into_iter().map(|target| (target, Ok(outcome))));
        targets = holding;
        if targets.is_empty() || Instant::now() >= until {
            return targets;
        }
        thread::sleep(POLL);
    }
}
