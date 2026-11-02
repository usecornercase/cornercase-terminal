use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
}

fn gone(name: &str) -> Error {
    Error::Probe(format!("{name} exited without answering"))
}

fn converse<T>(
    mut stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    first: &str,
    name: &str,
    mut talk: impl FnMut(&str) -> Step<T>,
) -> Result<T> {
    if stdin.write_all(first.as_bytes()).and_then(|()| stdin.flush()).is_err() {
        return Err(gone(name));
    }
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        match talk(&line) {
            Step::Read => {}
            Step::Write(text) => {
                if stdin.write_all(text.as_bytes()).and_then(|()| stdin.flush()).is_err() {
                    return Err(gone(name));
                }
            }
            Step::Answer(result) => return result,
        }
    }
    Err(gone(name))
}

pub(crate) fn run<T: Send + 'static>(
    limits: Limits,
    mut command: Command,
    first: &str,
    talk: impl FnMut(&str) -> Step<T> + Send + 'static,
) -> Result<T> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Probe(format!("could not run {program}: {e}")))?;
    let (tx, rx) = mpsc::channel();
    if let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) {
        let (first, name) = (first.to_string(), limits.name);
        std::thread::spawn(move || {
            let _ = tx.send(converse(stdin, stdout, &first, name, talk));
        });
    } else {
        drop(tx);
    }
    let answer = match rx.recv_timeout(limits.timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::Probe(format!("{} did not answer in time", limits.name))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(gone(limits.name)),
    };
    let _ = process::wait_until(&mut child, Instant::now() + limits.grace);
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
