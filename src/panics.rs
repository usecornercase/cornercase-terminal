use std::backtrace::Backtrace;
use std::collections::HashMap;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::thread;
use std::time::SystemTime;

use parking_lot::Mutex;

use crate::error::{Error, Result};
use crate::log::Stamp;

#[cfg(not(panic = "unwind"))]
compile_error!("the server contains panics with catch_unwind, so cornercase must be built with panic = \"unwind\"");

pub fn contain<T>(step: impl FnOnce() -> T) -> Option<T> {
    panic::catch_unwind(AssertUnwindSafe(step)).ok()
}

pub fn job<T>(work: impl FnOnce() -> Result<T>) -> Result<T> {
    contain(work).unwrap_or(Err(Error::Bug))
}

pub fn log_to_stderr() {
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
mod tests {
    use super::*;

    mod contain {
        use super::*;

        #[test]
        fn a_panic_drops_only_the_step_that_panicked() {
            let mut handled = Vec::new();
            for event in 1..=3 {
                contain(|| {
                    assert_ne!(event, 2, "a bug");
                    handled.push(event);
                });
            }
            assert_eq!(handled, [1, 3]);
        }

        #[test]
        fn gives_back_what_the_step_returns() {
            assert_eq!(contain(|| 7), Some(7));
        }
    }

    mod job {
        use super::*;

        #[test]
        fn a_panic_becomes_a_bug_to_show() {
            let result = job::<()>(|| panic!("a bug"));
            assert_eq!(result.map_err(|e| e.to_string()), Err("cornercase hit a bug, see server.log".into()));
        }

        #[test]
        fn keeps_the_answer_of_a_job_that_ends() {
            assert_eq!(job(|| Ok(7)).ok(), Some(7));
            assert!(matches!(job::<()>(|| Err(Error::Git("no upstream".into()))), Err(Error::Git(_))));
        }
    }

    mod reports {
        use super::*;

        fn report(reports: &mut Reports, location: &str) -> Option<String> {
            reports.report("main", location, "min > max", || "   0: cornercase::ui::draw\n".into())
        }

        #[test]
        fn the_first_panic_at_a_place_comes_with_its_backtrace() {
            assert_eq!(
                report(&mut Reports::default(), "src/ui.rs:12:5").as_deref(),
                Some("thread 'main' panicked at src/ui.rs:12:5:\nmin > max\n   0: cornercase::ui::draw")
            );
        }

        #[test]
        fn repeats_are_told_ever_more_rarely() {
            let mut reports = Reports::default();
            let told: Vec<usize> = (1..=20).filter(|_| report(&mut reports, "src/ui.rs:12:5").is_some()).collect();
            assert_eq!(told, [1, 2, 4, 8, 16]);
        }

        #[test]
        fn a_repeat_says_how_often_it_happened() {
            let mut reports = Reports::default();
            report(&mut reports, "src/ui.rs:12:5");
            assert_eq!(
                report(&mut reports, "src/ui.rs:12:5").as_deref(),
                Some("thread 'main' panicked at src/ui.rs:12:5 again, 2 times so far:\nmin > max")
            );
        }

        #[test]
        fn each_place_is_counted_on_its_own() {
            let mut reports = Reports::default();
            report(&mut reports, "src/ui.rs:12:5");
            let other = report(&mut reports, "src/app.rs:7:9");
            assert!(other.is_some_and(|text| text.contains("src/app.rs:7:9:\n")));
        }
    }
}
