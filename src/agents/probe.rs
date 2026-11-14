use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::error::{Error, Result};
use crate::process;

pub(crate) enum Step<T> {
    Read,
    Write(String),
    Answer(Result<T>),
}

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub(crate) name: &'static str,
    pub(crate) timeout: Duration,
    pub(crate) grace: Duration,
    pub(crate) close_input: bool,
}

fn gone(name: &str) -> Error {
    Error::Probe(format!("{name} exited without answering"))
}

fn send(stdin: Option<&mut std::process::ChildStdin>, text: &str) -> bool {
    stdin.is_some_and(|stdin| stdin.write_all(text.as_bytes()).and_then(|()| stdin.flush()).is_ok())
}

fn converse<T>(
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    (first, name, close): (&str, &str, bool),
    mut talk: impl FnMut(&str) -> Step<T>,
) -> Result<T> {
    let mut stdin = Some(stdin);
    if !send(stdin.as_mut(), first) {
        return Err(gone(name));
    }
    if close {
        stdin = None;
    }
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        match talk(&line) {
            Step::Read => {}
            Step::Write(text) => {
                if !send(stdin.as_mut(), &text) {
                    return Err(gone(name));
                }
            }
            Step::Answer(result) => return result,
        }
    }
    Err(gone(name))
}

#[derive(Debug, Default)]
struct Spawned {
    pid: Option<(i32, Option<u64>)>,
    stopped: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Running(Arc<Mutex<Spawned>>);

impl Running {
    pub(crate) fn stop(&self) -> bool {
        let pid = {
            let mut spawned = self.0.lock();
            spawned.stopped = true;
            spawned.pid
        };
        let Some((pid, started)) = pid else { return false };
        crate::processes::stop::stop(crate::processes::stop::Group::live_with(pid, started));
        true
    }

    fn spawned(&self, pid: u32) -> bool {
        let Ok(pid) = i32::try_from(pid) else { return true };
        let mut spawned = self.0.lock();
        spawned.pid = Some((pid, process::start_stamp(pid)));
        !spawned.stopped
    }
}

pub(crate) fn run<T: Send + 'static>(
    limits: Limits,
    command: Command,
    first: &str,
    talk: impl FnMut(&str) -> Step<T> + Send + 'static,
) -> Result<T> {
    run_in(limits, command, first, &Running::default(), talk)
}

pub(crate) fn run_in<T: Send + 'static>(
    limits: Limits,
    mut command: Command,
    first: &str,
    running: &Running,
    talk: impl FnMut(&str) -> Step<T> + Send + 'static,
) -> Result<T> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Probe(format!("could not run {program}: {e}")))?;
    if !running.spawned(child.id()) {
        running.stop();
    }
    let (tx, rx) = mpsc::channel();
    if let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) {
        let (first, name, close) = (first.to_string(), limits.name, limits.close_input);
        std::thread::spawn(move || {
            let _ = tx.send(converse(stdin, stdout, (&first, name, close), talk));
        });
    } else {
        drop(tx);
    }
    let answer = match rx.recv_timeout(limits.timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            running.stop();
            Err(Error::Probe(format!("{} did not answer in time", limits.name)))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(gone(limits.name)),
    };
    if answer.is_ok() {
        let _ = process::wait_until(&mut child, Instant::now() + limits.grace);
    }
    let _ = child.kill();
    let _ = child.wait();
    answer
}

#[derive(Debug)]
pub(crate) struct Probe<T> {
    last: Option<(T, Instant)>,
    loading: bool,
    error: Option<String>,
    answers: u64,
}

impl<T> Default for Probe<T> {
    fn default() -> Self {
        Self { last: None, loading: false, error: None, answers: 0 }
    }
}

impl<T> Probe<T> {
    pub(crate) const UNASKED: Self = Self { last: None, loading: false, error: None, answers: 0 };

    pub(crate) fn start(&mut self) -> bool {
        if self.loading {
            return false;
        }
        self.loading = true;
        self.error = None;
        true
    }

    pub(crate) fn answered(&mut self, result: Result<T>, now: Instant) {
        self.loading = false;
        self.answers += 1;
        match result {
            Ok(report) => self.last = Some((report, now)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub(crate) fn last(&self) -> Option<&(T, Instant)> {
        self.last.as_ref()
    }

    pub(crate) fn answers(&self) -> u64 {
        self.answers
    }

    pub(crate) fn loading(&self) -> bool {
        self.loading
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[cfg(test)]
mod tests;
