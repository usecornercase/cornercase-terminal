use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use super::link::{self, Connect, Ends, Heard};
use crate::control::Command;
use crate::remote::Killer;

const LATE: &str = "it did not answer in time";

#[derive(Default)]
struct Watch {
    passed: bool,
    ends: Option<Arc<dyn Ends>>,
}

#[derive(Default)]
struct Deadline {
    watch: Mutex<Watch>,
    killer: Killer,
}

impl Deadline {
    fn pass(&self) {
        let mut watch = self.watch.lock();
        watch.passed = true;
        self.killer.kill();
        if let Some(ends) = &watch.ends {
            ends.cut();
        }
    }

    fn hold(&self, ends: &Arc<dyn Ends>) {
        let mut watch = self.watch.lock();
        if watch.passed {
            ends.cut();
        }
        watch.ends = Some(Arc::clone(ends));
    }

    fn release(&self) {
        self.watch.lock().ends = None;
    }

    fn passed(&self) -> bool {
        self.watch.lock().passed
    }
}

pub(crate) fn ask(
    connect: &dyn Connect,
    name: &str,
    command: Command,
    boot: Option<String>,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Arc::new(Deadline::default());
    let (done, finished) = mpsc::channel::<()>();
    let watcher = Arc::clone(&deadline);
    thread::spawn(move || {
        if finished.recv_timeout(timeout) == Err(RecvTimeoutError::Timeout) {
            watcher.pass();
        }
    });
    let asked = asked(connect, name, command, boot, &deadline);
    drop(done);
    asked.map_err(|why| if deadline.passed() { LATE.into() } else { why })
}

fn asked(
    connect: &dyn Connect,
    name: &str,
    command: Command,
    boot: Option<String>,
    deadline: &Deadline,
) -> Result<String, String> {
    let mut line = connect.connect(&deadline.killer).map_err(|error| link::failure(&error, name))?;
    deadline.hold(&line.ends);
    let heard = link::send_once(&mut line, command, boot);
    deadline.release();
    match heard {
        Heard::Answer(text) => Ok(text),
        Heard::Ended(said) => Err(link::why(&line, said)),
    }
}

#[cfg(test)]
mod tests;
