use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::Event;
use rustix::event::{PollFd, PollFlags, Timespec};

use crate::host::notify;
use crate::protocol::ClientMessage;
use crate::{log, panics, paths};

mod parse;

const RETRY: Duration = Duration::from_millis(20);
const REPLY_WAIT: Duration = Duration::from_secs(1);
const REPLY_PREFIXES: &[&[u8]] = &[b"\x1b]4;", b"\x1b]10;", b"\x1b]11;", b"\x1bP>|", b"\x1b_Gi=", b"\x1b]99;"];
const ESC_WAIT: Duration = Duration::from_millis(100);
const SEQUENCE_LIMIT: usize = 4096;

type Decode<'a> = dyn FnMut(&[u8], bool) -> io::Result<Option<Decoded>> + 'a;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Input {
    Event(Event),
    Activated(String),
}

impl From<Input> for ClientMessage {
    fn from(input: Input) -> Self {
        match input {
            Input::Event(event) => Self::Event(event),
            Input::Activated(token) => Self::Activated(token),
        }
    }
}

enum Absorbed {
    Yes,
    No,
    Stop,
}

pub(super) struct Source<P: FnMut(Duration) -> io::Result<bool>, R: FnMut(&mut [u8]) -> io::Result<usize>> {
    pub(super) poll: P,
    pub(super) read: R,
}

pub(super) enum Decoded {
    Event(Event),
    Reply,
}

#[derive(Default)]
pub(crate) struct Parser {
    pending: Vec<u8>,
    since: Option<Instant>,
    replies_until: Option<Instant>,
    failures: u64,
}

impl Parser {
    fn new(probe_timed_out: bool) -> Self {
        Self { replies_until: probe_timed_out.then(|| Instant::now() + REPLY_WAIT), ..Self::default() }
    }

    pub(crate) fn feed(&mut self, bytes: &[u8], full: bool, emit: &mut impl FnMut(Input) -> bool) -> bool {
        self.feed_with(bytes, full, emit, &mut parse::parse_event)
    }

    fn feed_with(
        &mut self,
        bytes: &[u8],
        full: bool,
        emit: &mut impl FnMut(Input) -> bool,
        decode: &mut Decode<'_>,
    ) -> bool {
        if bytes.first() == Some(&0x1b) && self.pending == b"\x1b" {
            self.reset();
            if !emit(Input::Event(Event::Key(crossterm::event::KeyCode::Esc.into()))) {
                return false;
            }
        }
        for (at, &byte) in bytes.iter().enumerate() {
            let more = at + 1 < bytes.len() || full;
            let Some(remainder) = self.feed_byte(byte, more, emit, decode) else { return false };
            if !remainder.is_empty() && !self.feed_with(&remainder, full, emit, decode) {
                return false;
            }
        }
        true
    }

    fn feed_byte(
        &mut self,
        byte: u8,
        more: bool,
        emit: &mut impl FnMut(Input) -> bool,
        decode: &mut Decode<'_>,
    ) -> Option<Vec<u8>> {
        self.pending.push(byte);
        self.since = Some(Instant::now());
        self.drop_before_next_escape();
        match self.absorb_reply(more, emit) {
            Absorbed::Yes => return Some(Vec::new()),
            Absorbed::Stop => return None,
            Absorbed::No => {}
        }
        if byte == 0x1b && self.pending.len() > 2 && !self.pending.starts_with(b"\x1b[200~") {
            self.pending.clear();
            self.pending.push(byte);
        }
        let remainder = self.split_remainder();
        let more = more || (self.pending == b"\x1b" && self.expects_replies());
        let result = panics::contain(|| decode(&self.pending, more));
        self.apply_decoded(result, emit).then_some(remainder)
    }

    fn drop_before_next_escape(&mut self) {
        if REPLY_PREFIXES.iter().any(|prefix| self.pending.starts_with(prefix))
            && let Some(at) = self
                .pending
                .windows(2)
                .enumerate()
                .skip(2)
                .find_map(|(at, pair)| (pair[0] == 0x1b && pair[1] != b'\\').then_some(at))
        {
            self.pending.drain(..at);
            self.since = Some(Instant::now());
        }
    }

    fn absorb_reply(&mut self, more: bool, emit: &mut impl FnMut(Input) -> bool) -> Absorbed {
        if self.pending.starts_with(b"\x1b[200~") {
            return Absorbed::No;
        }
        let Some(end) = reply(&self.pending) else { return Absorbed::No };
        if !(end || !reply_prefix(&self.pending) || more || self.expects_replies()) {
            return Absorbed::No;
        }
        if end {
            self.replies_until = Some(Instant::now() + REPLY_WAIT);
            let activated = notify::activation(&self.pending);
            self.reset();
            if let Some(token) = activated
                && !emit(Input::Activated(token))
            {
                return Absorbed::Stop;
            }
        }
        Absorbed::Yes
    }

    fn split_remainder(&mut self) -> Vec<u8> {
        if self.pending.len() > 2 && matches!(self.pending[..2], [0x1b, b']' | b'P' | b'_']) {
            self.pending.split_off(2)
        } else {
            Vec::new()
        }
    }

    fn apply_decoded(
        &mut self,
        result: Option<io::Result<Option<Decoded>>>,
        emit: &mut impl FnMut(Input) -> bool,
    ) -> bool {
        match result {
            Some(Ok(Some(Decoded::Event(event)))) => {
                self.reset();
                return emit(Input::Event(event));
            }
            Some(Ok(Some(Decoded::Reply))) => {
                self.replies_until = Some(Instant::now() + REPLY_WAIT);
                self.reset();
            }
            Some(Err(_)) => self.reset(),
            Some(Ok(None)) => {
                if self.pending.len() > SEQUENCE_LIMIT && !self.pending.starts_with(b"\x1b[200~") {
                    self.reset();
                }
            }
            None => {
                self.reset();
                self.failures += 1;
                if self.failures.is_power_of_two() {
                    report("input parser panicked; discarded the sequence");
                }
            }
        }
        true
    }

    pub(crate) fn idle(&mut self, emit: &mut impl FnMut(Input) -> bool) -> bool {
        if self.since.is_some_and(|since| since.elapsed() >= ESC_WAIT) && self.pending == b"\x1b" {
            self.reset();
            return emit(Input::Event(Event::Key(crossterm::event::KeyCode::Esc.into())));
        }
        if self.since.is_some_and(|since| since.elapsed() >= ESC_WAIT) && reply_prefix(&self.pending) {
            let bytes = std::mem::take(&mut self.pending);
            self.since = None;
            if let Some(Ok(Some(Decoded::Event(event)))) = panics::contain(|| parse::parse_event(&bytes[..2], false))
                && !emit(Input::Event(event))
            {
                return false;
            }
            return self.feed(&bytes[2..], false, emit);
        }
        if self.since.is_some_and(|since| since.elapsed() >= REPLY_WAIT) && reply(&self.pending).is_some() {
            self.reset();
        }
        true
    }

    fn expects_replies(&self) -> bool {
        self.replies_until.is_some_and(|until| Instant::now() < until)
    }

    fn wait(&self) -> Duration {
        let hold = if self.pending == b"\x1b" || reply_prefix(&self.pending) {
            ESC_WAIT
        } else if reply(&self.pending).is_some() {
            REPLY_WAIT
        } else {
            return super::INPUT_POLL;
        };
        self.since.map_or(super::INPUT_POLL, |since| hold.saturating_sub(since.elapsed()).min(super::INPUT_POLL))
    }

    pub(super) fn reset(&mut self) {
        self.pending.clear();
        self.since = None;
    }
}

fn reply_prefix(bytes: &[u8]) -> bool {
    bytes.len() > 1 && REPLY_PREFIXES.iter().any(|prefix| prefix.starts_with(bytes) && prefix.len() > bytes.len())
}

fn reply(bytes: &[u8]) -> Option<bool> {
    if bytes.starts_with(b"\x1b[?") || bytes.starts_with(b"\x1b[>") {
        if bytes[2..].contains(&0x1b) || bytes.len() > SEQUENCE_LIMIT {
            return None;
        }
        return Some(bytes.last().is_some_and(|last| (b'@'..=b'~').contains(last)));
    }
    if REPLY_PREFIXES.iter().any(|prefix| bytes.starts_with(prefix)) {
        if bytes.len() > SEQUENCE_LIMIT || bytes.ends_with(b"\x07") || bytes.ends_with(b"\x1b\\") {
            return Some(true);
        }
        if bytes.last() == Some(&0x1b) || !bytes[2..].contains(&0x1b) {
            return Some(false);
        }
        return None;
    }
    if bytes.len() > 1 && REPLY_PREFIXES.iter().any(|prefix| prefix.starts_with(bytes)) {
        return Some(false);
    }
    None
}

pub(super) fn readable(input: &impl AsFd, wait: Duration) -> io::Result<bool> {
    let wait = Timespec::try_from(wait).map_err(io::Error::other)?;
    let mut fds = [PollFd::new(input, PollFlags::IN)];
    rustix::event::poll(&mut fds, Some(&wait))?;
    let flags = fds[0].revents();
    if flags.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(flags.contains(PollFlags::IN))
}

pub(super) fn run(
    stop: &AtomicBool,
    probe_timed_out: bool,
    mut source: Source<impl FnMut(Duration) -> io::Result<bool>, impl FnMut(&mut [u8]) -> io::Result<usize>>,
    mut emit: impl FnMut(Input) -> bool,
    mut pause: impl FnMut(Duration),
) {
    let mut parser = Parser::new(probe_timed_out);
    let mut bytes = [0; 1024];
    let mut failures = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let result =
            panics::contain(
                || if (source.poll)(parser.wait())? { (source.read)(&mut bytes).map(Some) } else { Ok(None) },
            );
        match result {
            Some(Ok(Some(0))) => return,
            Some(Err(error)) if error.kind() == io::ErrorKind::Interrupted => {}
            Some(Err(error)) if ended(&error) => return,
            Some(Ok(Some(n))) => {
                if !parser.feed(&bytes[..n], n == bytes.len(), &mut emit) {
                    return;
                }
            }
            Some(Ok(None)) => {
                if !parser.idle(&mut emit) {
                    return;
                }
            }
            failed => {
                parser.reset();
                failures += 1;
                if failures.is_power_of_two() {
                    let reason = match failed {
                        Some(Err(error)) => error.to_string(),
                        _ => "input reader panicked".into(),
                    };
                    report(&format!("{reason}; retrying input"));
                }
                pause(RETRY);
            }
        }
    }
}

fn ended(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected)
        || error.raw_os_error().is_some_and(|code| {
            code == rustix::io::Errno::IO.raw_os_error()
                || code == rustix::io::Errno::BADF.raw_os_error()
                || code == rustix::io::Errno::NODEV.raw_os_error()
        })
}

pub(super) fn report(message: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).mode(log::PRIVATE).open(paths::log()) {
        let line = log::line(SystemTime::now(), log::Level::Warn, "client", message, &[]);
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests;
