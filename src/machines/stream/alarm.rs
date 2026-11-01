use std::mem;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use crate::machines::link::Ends;

pub(super) trait Bell: Send + Sync {
    fn arm(&self, armed: Arc<Armed>, ends: Arc<dyn Ends>, wait: Duration);
}

struct Due {
    until: Instant,
    armed: Arc<Armed>,
    ends: Arc<dyn Ends>,
}

#[derive(Default)]
struct Watch {
    due: Vec<Due>,
    stopped: bool,
}

impl Watch {
    fn late(&mut self, now: Instant) -> Vec<Due> {
        let (late, waiting) =
            mem::take(&mut self.due).into_iter().filter(|due| !due.armed.is_off()).partition(|due| due.until <= now);
        self.due = waiting;
        late
    }
}

#[derive(Default)]
struct Dog {
    watch: Mutex<Watch>,
    wake: Condvar,
}

impl Dog {
    fn run(&self) {
        let mut watch = self.watch.lock();
        while !watch.stopped {
            let late = watch.late(Instant::now());
            if !late.is_empty() {
                drop(watch);
                late.iter().filter(|due| due.armed.ring()).for_each(|due| due.ends.cut());
                watch = self.watch.lock();
                continue;
            }
            match watch.due.iter().map(|due| due.until).min() {
                Some(until) => {
                    self.wake.wait_until(&mut watch, until);
                }
                None => self.wake.wait(&mut watch),
            }
        }
    }
}

pub(super) struct Timer {
    dog: Arc<Dog>,
}

impl Timer {
    pub(super) fn new() -> Self {
        let dog = Arc::new(Dog::default());
        let watching = Arc::clone(&dog);
        thread::spawn(move || watching.run());
        Self { dog }
    }
}

impl Bell for Timer {
    fn arm(&self, armed: Arc<Armed>, ends: Arc<dyn Ends>, wait: Duration) {
        self.dog.watch.lock().due.push(Due { until: Instant::now() + wait, armed, ends });
        self.dog.wake.notify_all();
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.dog.watch.lock().stopped = true;
        self.dog.wake.notify_all();
    }
}

#[derive(Default)]
struct Ringing {
    off: bool,
    rung: bool,
}

#[derive(Default)]
pub(super) struct Armed {
    state: Mutex<Ringing>,
    wake: Condvar,
}

impl Armed {
    #[cfg(test)]
    pub(super) fn wait(&self) -> bool {
        let mut state = self.state.lock();
        while !state.off && !state.rung {
            self.wake.wait(&mut state);
        }
        state.rung
    }

    pub(super) fn ring(&self) -> bool {
        let mut state = self.state.lock();
        state.rung |= !state.off;
        self.wake.notify_all();
        state.rung
    }

    fn off(&self) {
        self.state.lock().off = true;
        self.wake.notify_all();
    }

    fn is_off(&self) -> bool {
        self.state.lock().off
    }

    fn rang(&self) -> bool {
        self.state.lock().rung
    }
}

pub(super) struct Alarms {
    bell: Arc<dyn Bell>,
    within: Duration,
}

impl Alarms {
    pub(super) fn new(bell: Arc<dyn Bell>, within: Duration) -> Self {
        Self { bell, within }
    }

    pub(super) fn set(&self, ends: Arc<dyn Ends>) -> Alarm {
        let armed = Arc::new(Armed::default());
        self.bell.arm(Arc::clone(&armed), ends, self.within);
        Alarm { armed }
    }
}

pub(super) struct Alarm {
    armed: Arc<Armed>,
}

impl Alarm {
    pub(super) fn off(&self) {
        self.armed.off();
    }

    pub(super) fn rang(&self) -> bool {
        self.armed.rang()
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        self.armed.off();
    }
}

#[cfg(test)]
mod tests;
