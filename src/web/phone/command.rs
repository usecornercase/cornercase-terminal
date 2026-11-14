use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::Instant;

use parking_lot::{Condvar, Mutex};

use super::Limits;
use super::message::{Input, Output};
use crate::control::{PANE_ENV, SERVER_ENV};
use crate::os::process::{ExitStatusExt as _, Grouped, Signal};
use crate::paths;
use crate::process;
use crate::protocol::{REMOTE_ENV, RESTARTED_ENV, TIED_ENV};

const CHUNK: usize = 8192;
const STDIN_CHUNKS: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Out {
    Line(String),
    TooLong,
    End,
}

impl Out {
    fn size(&self) -> usize {
        match self {
            Self::Line(text) => text.len(),
            Self::TooLong | Self::End => 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Writing {
    Sent,
    Full,
    Gone,
}

struct State {
    items: VecDeque<Out>,
    bytes: usize,
    closed: bool,
}

pub(super) struct Queue {
    state: Mutex<State>,
    room: Condvar,
    limit: usize,
    waits: Arc<AtomicUsize>,
}

impl Queue {
    pub(super) fn new(limit: usize, waits: Arc<AtomicUsize>) -> Self {
        let state = State { items: VecDeque::new(), bytes: 0, closed: false };
        Self { state: Mutex::new(state), room: Condvar::new(), limit, waits }
    }

    pub(super) fn push(&self, out: Out) -> bool {
        let size = out.size();
        let mut state = self.state.lock();
        while !state.closed && state.bytes > 0 && state.bytes + size > self.limit {
            self.waits.fetch_add(1, Ordering::SeqCst);
            self.room.wait(&mut state);
        }
        if state.closed {
            return false;
        }
        state.bytes += size;
        state.items.push_back(out);
        true
    }

    pub(super) fn pop(&self) -> Option<Out> {
        let mut state = self.state.lock();
        let out = state.items.pop_front()?;
        state.bytes -= out.size();
        self.room.notify_all();
        Some(out)
    }

    pub(super) fn close(&self) {
        self.state.lock().closed = true;
        self.room.notify_all();
    }

    #[cfg(test)]
    pub(super) fn bytes(&self) -> usize {
        self.state.lock().bytes
    }
}

pub(super) fn command(program: &Path, argv: &[String], socket: &Path) -> Command {
    let mut command = Command::new(program);
    command.args(argv);
    for variable in [REMOTE_ENV, PANE_ENV, SERVER_ENV, RESTARTED_ENV, TIED_ENV] {
        command.env_remove(variable);
    }
    command.env(paths::SOCKET_ENV, socket);
    if let Some(home) = std::env::var_os(paths::HOME_VAR) {
        command.current_dir(home);
    }
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

pub(super) fn exit_code(code: Option<i32>, signal: Option<i32>) -> i32 {
    code.or_else(|| signal.map(|signal| 128 + signal)).unwrap_or(-1)
}

struct Splitter {
    line: Vec<u8>,
    limit: usize,
}

impl Splitter {
    fn feed(&mut self, chunk: &[u8], found: &mut Vec<String>) -> bool {
        for part in chunk.split_inclusive(|byte| *byte == b'\n') {
            let (body, complete) = match part.split_last() {
                Some((b'\n', body)) => (body, true),
                _ => (part, false),
            };
            if self.line.len() + body.len() > self.limit {
                return false;
            }
            self.line.extend_from_slice(body);
            if complete {
                found.push(String::from_utf8_lossy(&self.line).into_owned());
                self.line.clear();
            }
        }
        true
    }

    fn rest(&self) -> Option<String> {
        (!self.line.is_empty()).then(|| String::from_utf8_lossy(&self.line).into_owned())
    }
}

pub(super) fn lines(mut pipe: impl Read, limit: usize, mut push: impl FnMut(Out) -> bool, wrap: fn(String) -> Output) {
    let mut splitter = Splitter { line: Vec::new(), limit };
    let (mut buffer, mut found) = ([0u8; CHUNK], Vec::new());
    let mut fits = true;
    while fits {
        let read = match pipe.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        fits = splitter.feed(&buffer[..read], &mut found);
        if !found.drain(..).all(|text| push(Out::Line(wrap(text).json()))) {
            return;
        }
    }
    let last = if fits { splitter.rest() } else { None };
    if !fits && !push(Out::TooLong) || last.is_some_and(|text| !push(Out::Line(wrap(text).json()))) {
        return;
    }
    let _ = push(Out::End);
}

pub(super) struct Running {
    child: Grouped,
    stdin: Option<SyncSender<Vec<u8>>>,
    pending: Option<Vec<u8>>,
    queue: Arc<Queue>,
    ends: u8,
    limits: Limits,
    exited: Option<(ExitStatus, Instant)>,
    killed: bool,
}

impl Running {
    pub(super) fn start(mut command: Command, limits: Limits, waits: &Arc<AtomicUsize>) -> io::Result<Self> {
        let mut child = Grouped::spawn(&mut command)?;
        let queue = Arc::new(Queue::new(limits.queued_bytes, Arc::clone(waits)));
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, &queue, limits.longest_line, Output::Stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, &queue, limits.longest_line, Output::Stderr);
        }
        let stdin = child.stdin.take().map(|pipe| {
            let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(STDIN_CHUNKS);
            thread::spawn(move || feed(pipe, &rx));
            tx
        });
        Ok(Self { child, stdin, pending: None, queue, ends: 0, limits, exited: None, killed: false })
    }

    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn write(&mut self, input: Input) -> Writing {
        let Some(bytes) = input.into_bytes() else {
            self.stdin = None;
            return Writing::Sent;
        };
        self.send(bytes)
    }

    fn send(&mut self, bytes: Vec<u8>) -> Writing {
        let Some(stdin) = &self.stdin else { return Writing::Gone };
        match stdin.try_send(bytes) {
            Ok(()) => Writing::Sent,
            Err(TrySendError::Full(bytes)) => {
                self.pending = Some(bytes);
                Writing::Full
            }
            Err(TrySendError::Disconnected(_)) => Writing::Gone,
        }
    }

    pub(super) fn blocked(&mut self) -> bool {
        if let Some(bytes) = self.pending.take() {
            self.send(bytes);
        }
        self.pending.is_some()
    }

    pub(super) fn next(&mut self) -> Option<Out> {
        loop {
            match self.queue.pop()? {
                Out::End => self.ends += 1,
                out => return Some(out),
            }
        }
    }

    pub(super) fn finished(&mut self) -> Option<i32> {
        if self.exited.is_none() {
            self.exited = self.child.try_wait().ok().flatten().map(|status| (status, Instant::now()));
        }
        let (status, since) = self.exited?;
        if self.ends < 2 && !self.killed && since.elapsed() >= self.limits.grace {
            self.killed = true;
            self.child.signal(Signal::KILL);
        }
        (self.ends >= 2).then(|| exit_code(status.code(), status.signal()))
    }

    pub(super) fn kill(&mut self) {
        self.stdin = None;
        process::kill_group(&mut self.child);
        self.queue.close();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.queue.close();
    }
}

fn feed(mut pipe: ChildStdin, chunks: &Receiver<Vec<u8>>) {
    for chunk in chunks {
        if pipe.write_all(&chunk).and_then(|()| pipe.flush()).is_err() {
            return;
        }
    }
}

fn spawn_reader(pipe: impl Read + Send + 'static, queue: &Arc<Queue>, limit: usize, wrap: fn(String) -> Output) {
    let queue = Arc::clone(queue);
    thread::spawn(move || lines(pipe, limit, |out| queue.push(out), wrap));
}

#[cfg(test)]
mod tests;
