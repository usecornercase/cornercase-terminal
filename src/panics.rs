use std::backtrace::Backtrace;
use std::collections::HashMap;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::thread;
use std::time::SystemTime;

use parking_lot::Mutex;

use crate::error::{Error, Result};
use crate::time::Stamp;

#[cfg(not(panic = "unwind"))]
compile_error!("the server contains panics with catch_unwind, so cornercase must be built with panic = \"unwind\"");

pub(crate) fn contain<T>(step: impl FnOnce() -> T) -> Option<T> {
    panic::catch_unwind(AssertUnwindSafe(step)).ok()
}

pub(crate) fn job<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    contain(work).unwrap_or(Err(Error::Bug))
}

pub(crate) fn log_to_stderr() {
    let reports = Mutex::new(Reports::default());
    panic::set_hook(Box::new(move |info| {
        let thread = thread::current();
        let location = info.location().map_or_else(|| "an unknown place".into(), ToString::to_string);
        let message = info.payload_as_str().unwrap_or("no message");
        let backtrace = || Backtrace::force_capture().to_string();
        if let Some(report) = reports.lock().report(thread.name().unwrap_or("unnamed"), &location, message, backtrace) {
            let _ = writeln!(io::stderr().lock(), "{} ERROR panic: {report}", Stamp(SystemTime::now()));
        }
    }));
}

#[derive(Debug, Default)]
struct Reports(HashMap<String, u64>);

impl Reports {
    fn report(
        &mut self,
        thread: &str,
        location: &str,
        message: &str,
        backtrace: impl FnOnce() -> String,
    ) -> Option<String> {
        let seen = self.0.entry(location.to_string()).or_default();
        *seen += 1;
        let head = format!("thread '{thread}' panicked at {location}");
        match *seen {
            1 => Some(format!("{head}:\n{message}\n{}", backtrace().trim_end())),
            n if n.is_power_of_two() => Some(format!("{head} again, {n} times so far:\n{message}")),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
