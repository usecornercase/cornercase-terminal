use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::error::{Error, Result};
use crate::os::process::Lineage;
use crate::process;
use crate::processes::stop::{self, Group};

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
    pub(crate) contain: bool,
    pub(crate) stderr: bool,
}

const REASON_WAIT: Duration = Duration::from_millis(500);

fn gone(name: &str, reason: Option<&mpsc::Receiver<String>>) -> Error {
    match reason.and_then(|reason| reason.recv_timeout(REASON_WAIT).ok()) {
        Some(why) => Error::Probe(format!("{name} exited without answering: {why}")),
        None => Error::Probe(format!("{name} exited without answering")),
    }
}

fn first_reason(stderr: &mut impl BufRead) -> Option<String> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if stderr.read_until(b'\n', &mut line).ok()? == 0 {
            return None;
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if !text.is_empty() {
            return Some(crate::text::truncate_right(&crate::text::one_line(text), 200));
        }
    }
}

fn reason_of(stderr: std::process::ChildStderr) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stderr = BufReader::new(stderr);
        if let Some(reason) = first_reason(&mut stderr) {
            let _ = tx.send(reason);
        }
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
    });
    rx
}

fn send(stdin: Option<&mut std::process::ChildStdin>, text: &str) -> bool {
    stdin.is_some_and(|stdin| stdin.write_all(text.as_bytes()).and_then(|()| stdin.flush()).is_ok())
}

type Talked<T> = (Result<T>, Vec<Group>);

struct Pipes {
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    reason: Option<mpsc::Receiver<String>>,
}

fn converse<T>(
    pipes: Pipes,
    (first, name, close): (&str, &str, bool),
    mut talk: impl FnMut(&str) -> Step<T>,
    left: impl Fn() -> Vec<Group>,
) -> Talked<T> {
    let Pipes { stdin, stdout, reason } = pipes;
    let mut stdin = Some(stdin);
    if !send(stdin.as_mut(), first) {
        return (Err(gone(name, reason.as_ref())), Vec::new());
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
                    return (Err(gone(name, reason.as_ref())), Vec::new());
                }
            }
            Step::Answer(result) => return (result, left()),
        }
    }
    (Err(gone(name, reason.as_ref())), Vec::new())
}

#[derive(Debug, Default)]
struct Spawned {
    pid: Option<(i32, Option<u64>)>,
    stopped: bool,
    lineage: Option<Lineage>,
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
        stop::stop(Group::live_with(pid, started));
        self.end_lineage();
        true
    }

    pub(crate) fn holds(&self, pid: i32) -> bool {
        let spawned = self.0.lock();
        let (Some((root, started)), Some(lineage)) = (spawned.pid, &spawned.lineage) else { return false };
        process::running(root, started) && (pid == root || stop::inside(pid, root) || lineage.holds(pid))
    }

    pub(crate) fn contained(&self) -> bool {
        let spawned = self.0.lock();
        spawned.lineage.is_some() && spawned.pid.is_some_and(|(root, started)| process::running(root, started))
    }

    fn end_lineage(&self) {
        let lineage = self.0.lock().lineage.take();
        if let Some(lineage) = lineage {
            lineage.end();
        }
    }

    fn spawned(&self, pid: u32, lineage: Option<Lineage>) -> bool {
        let Ok(pid) = i32::try_from(pid) else { return true };
        let mut spawned = self.0.lock();
        spawned.pid = Some((pid, process::start_stamp(pid)));
        spawned.lineage = lineage;
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

fn start(limits: Limits, command: &mut Command, running: &Running) -> Result<Child> {
    let program = command.get_program().to_string_lossy().into_owned();
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(if limits.stderr {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let spawned = if limits.contain {
        Lineage::spawn(command).map(|(child, lineage)| (child, Some(lineage)))
    } else {
        command.spawn().map(|child| (child, None))
    };
    let (child, lineage) = spawned.map_err(|e| Error::Probe(format!("could not run {program}: {e}")))?;
    if !running.spawned(child.id(), lineage) {
        running.stop();
    }
    Ok(child)
}

fn converse_apart<T: Send + 'static>(
    child: &mut Child,
    limits: Limits,
    first: &str,
    talk: impl FnMut(&str) -> Step<T> + Send + 'static,
) -> mpsc::Receiver<Talked<T>> {
    let (tx, rx) = mpsc::channel();
    let (root, contain) = (i32::try_from(child.id()).unwrap_or(0), limits.contain);
    if let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) {
        let (first, name, close) = (first.to_string(), limits.name, limits.close_input);
        let reason = child.stderr.take().map(reason_of);
        std::thread::spawn(move || {
            let left = || if contain { stop::below(root) } else { Vec::new() };
            let pipes = Pipes { stdin, stdout, reason };
            let _ = tx.send(converse(pipes, (&first, name, close), talk, left));
        });
    }
    rx
}

pub(crate) fn run_in<T: Send + 'static>(
    limits: Limits,
    mut command: Command,
    first: &str,
    running: &Running,
    talk: impl FnMut(&str) -> Step<T> + Send + 'static,
) -> Result<T> {
    let mut child = start(limits, &mut command, running)?;
    let rx = converse_apart(&mut child, limits, first, talk);
    let (answer, left) = match rx.recv_timeout(limits.timeout) {
        Ok(talked) => talked,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            running.stop();
            (Err(Error::Probe(format!("{} did not answer in time", limits.name))), Vec::new())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => (Err(gone(limits.name, None)), Vec::new()),
    };
    let root = i32::try_from(child.id()).unwrap_or(0);
    let mut left = left;
    if limits.contain && left.is_empty() {
        left = stop::below(root);
    }
    if answer.is_ok() {
        let exited = process::wait_until(&mut child, Instant::now() + limits.grace);
        if limits.contain && matches!(exited, Ok(None)) {
            stop::add_below(&mut left, root);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if limits.contain {
        stop::stop_all(left);
        running.end_lineage();
    }
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
