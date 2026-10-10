use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use crossterm::event::Event;
use rustix::event::{PollFd, PollFlags, Timespec};

use crate::{log, panics};

mod parse;

const RETRY: Duration = Duration::from_millis(20);
const REPLY_WAIT: Duration = Duration::from_secs(1);
const REPLY_PREFIXES: &[&[u8]] = &[b"\x1b]4;", b"\x1b]10;", b"\x1b]11;", b"\x1bP>|", b"\x1b_Gi="];
const ESC_WAIT: Duration = Duration::from_millis(100);
const SEQUENCE_LIMIT: usize = 4096;

type Decode<'a> = dyn FnMut(&[u8], bool) -> io::Result<Option<Decoded>> + 'a;

pub(super) enum Decoded {
    Event(Event),
    Reply,
}

#[derive(Default)]
pub(super) struct Parser {
    pending: Vec<u8>,
    since: Option<Instant>,
    failures: u64,
}

impl Parser {
    pub(super) fn feed(&mut self, bytes: &[u8], emit: &mut impl FnMut(Event) -> bool) -> bool {
        self.feed_with(bytes, emit, &mut parse::parse_event)
    }

    fn feed_with(&mut self, bytes: &[u8], emit: &mut impl FnMut(Event) -> bool, decode: &mut Decode<'_>) -> bool {
        for (at, &byte) in bytes.iter().enumerate() {
            self.pending.push(byte);
            self.since = Some(Instant::now());
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
            if !self.pending.starts_with(b"\x1b[200~")
                && let Some(end) = reply(&self.pending)
            {
                if end {
                    self.reset();
                }
                continue;
            }
            if byte == 0x1b && self.pending.len() > 1 && !self.pending.starts_with(b"\x1b[200~") {
                self.pending.clear();
                self.pending.push(byte);
            }
            let remainder = if self.pending.len() > 2 && matches!(self.pending[..2], [0x1b, b']' | b'P' | b'_']) {
                self.pending.split_off(2)
            } else {
                Vec::new()
            };
            let more = at + 1 < bytes.len() || self.pending == b"\x1b";
            let result = panics::contain(|| decode(&self.pending, more));
            match result {
                Some(Ok(Some(Decoded::Event(event)))) => {
                    self.reset();
                    if !emit(event) {
                        return false;
                    }
                }
                Some(Ok(Some(Decoded::Reply)) | Err(_)) => self.reset(),
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
            if !remainder.is_empty() && !self.feed_with(&remainder, emit, decode) {
                return false;
            }
        }
        true
    }

    pub(super) fn idle(&mut self, emit: &mut impl FnMut(Event) -> bool) -> bool {
        if self.since.is_some_and(|since| since.elapsed() >= ESC_WAIT) && self.pending == b"\x1b" {
            self.reset();
            return emit(Event::Key(crossterm::event::KeyCode::Esc.into()));
        }
        if self.since.is_some_and(|since| since.elapsed() >= ESC_WAIT)
            && self.pending.len() > 1
            && REPLY_PREFIXES
                .iter()
                .any(|prefix| prefix.starts_with(&self.pending) && prefix.len() > self.pending.len())
        {
            let bytes = std::mem::take(&mut self.pending);
            self.since = None;
            if let Some(Ok(Some(Decoded::Event(event)))) = panics::contain(|| parse::parse_event(&bytes[..2], false))
                && !emit(event)
            {
                return false;
            }
            return self.feed(&bytes[2..], emit);
        }
        if self.since.is_some_and(|since| since.elapsed() >= REPLY_WAIT) && reply(&self.pending).is_some() {
            self.reset();
        }
        true
    }

    pub(super) fn reset(&mut self) {
        self.pending.clear();
        self.since = None;
    }
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
    mut poll: impl FnMut() -> io::Result<bool>,
    mut read: impl FnMut(&mut [u8]) -> io::Result<usize>,
    mut emit: impl FnMut(Event) -> bool,
    mut pause: impl FnMut(Duration),
) {
    let mut parser = Parser::default();
    let mut bytes = [0; 1024];
    let mut failures = 0_u64;
    while !stop.load(Ordering::Relaxed) {
        let result = panics::contain(|| if poll()? { read(&mut bytes).map(Some) } else { Ok(None) });
        match result {
            Some(Ok(Some(0))) => return,
            Some(Err(error)) if ended(&error) => return,
            Some(Ok(Some(n))) => {
                if !parser.feed(&bytes[..n], &mut emit) {
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
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).mode(log::PRIVATE).open(log::path()) {
        let line = log::line(SystemTime::now(), log::Level::Warn, "client", message, &[]);
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::os::unix::net::UnixStream;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use rstest::rstest;

    use super::*;

    fn events(bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        Parser::default().feed(bytes, &mut |event| {
            events.push(event);
            true
        });
        events
    }

    fn key(c: char) -> Event {
        Event::Key(KeyCode::Char(c).into())
    }

    #[rstest]
    #[case::sixel(b"\x1b[?1;0;256S")]
    #[case::geometry(b"\x1b[?2;0;1000;570S")]
    #[case::attributes(b"\x1b[?62;22;52c")]
    #[case::secondary_attributes(b"\x1b[>1;400;0c")]
    #[case::cell_size(b"\x1b[6;34;16t")]
    #[case::color(b"\x1b]4;255;rgb:1111/2222/3333\x1b\\")]
    #[case::background(b"\x1b]11;rgb:00/00/00\x07")]
    #[case::terminal(b"\x1bP>|ghostty 1.3.1\x1b\\")]
    #[case::kitty(b"\x1b_Gi=31;OK\x1b\\")]
    fn late_replies_leave_the_next_click_and_key_alone(#[case] reply: &[u8]) {
        let click = b"\x1b[<0;8;5M";
        let stream = [reply, click, b"z"].concat();
        let expected = vec![
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 7,
                row: 4,
                modifiers: KeyModifiers::NONE,
            }),
            key('z'),
        ];
        assert_eq!(events(&stream), expected);
        for at in 1..stream.len() {
            let mut parser = Parser::default();
            let mut received = Vec::new();
            let mut emit = |event| {
                received.push(event);
                true
            };
            parser.feed(&stream[..at], &mut emit);
            parser.feed(&stream[at..], &mut emit);
            assert_eq!(received, expected, "split at {at}");
        }
    }

    #[rstest]
    #[case::sgr(b"\x1b[<0;0;0M")]
    #[case::rxvt(b"\x1b[32;0;0M")]
    #[case::normal(b"\x1b[M   ")]
    #[case::cursor(b"\x1b[0;0R")]
    fn zero_coordinates_are_discarded(#[case] sequence: &[u8]) {
        assert_eq!(events(&[sequence, b"z"].concat()), [key('z')]);
    }

    #[rstest]
    #[case::control_b(b"\x02", KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))]
    #[case::unicode("é".as_bytes(), KeyEvent::new(KeyCode::Char('é'), KeyModifiers::NONE))]
    #[case::alt_unicode("\x1bé".as_bytes(), KeyEvent::new(KeyCode::Char('é'), KeyModifiers::ALT))]
    #[case::up(b"\x1b[A", KeyCode::Up.into())]
    #[case::application_up(b"\x1bOA", KeyCode::Up.into())]
    #[case::function_key(b"\x1bOP", KeyCode::F(1).into())]
    #[case::backtab(b"\x1b[Z", KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))]
    #[case::control_enter(b"\x1b[13;5u", KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL))]
    #[case::shift_enter(b"\x1b[13;2u", KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT))]
    #[case::shift_up(b"\x1b[1;2A", KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT))]
    #[case::delete(b"\x1b[3~", KeyCode::Delete.into())]
    fn key_encodings_survive_split_reads(#[case] bytes: &[u8], #[case] expected: KeyEvent) {
        for size in 1..=bytes.len() {
            let mut parser = Parser::default();
            let mut received = Vec::new();
            for chunk in bytes.chunks(size) {
                parser.feed(chunk, &mut |event| {
                    received.push(event);
                    true
                });
            }
            assert_eq!(received, [Event::Key(expected)], "chunks of {size}");
        }
    }

    #[test]
    fn a_parser_panic_discards_its_bytes_and_keeps_the_rest_of_the_read() {
        let mut parser = Parser::default();
        let mut received = Vec::new();
        parser.feed_with(
            b"xz",
            &mut |event| {
                received.push(event);
                true
            },
            &mut |bytes, more| {
                assert_ne!(bytes, b"x", "a decoder bug");
                parse::parse_event(bytes, more)
            },
        );
        assert_eq!(received, [key('z')]);
        assert_eq!(parser.pending, b"");
    }

    #[test]
    fn replies_inside_a_paste_are_text() {
        let text = "hello \x1b]4;1;rgb:ff/00/00\x07 \x1b[?1;0;256S";
        let stream = format!("\x1b[200~{text}\x1b[201~");
        assert_eq!(events(stream.as_bytes()), [Event::Paste(text.into())]);
    }

    #[test]
    fn alt_keys_that_start_like_strings_are_preserved() {
        for c in [']', 'P', '_'] {
            let stream = format!("\x1b{c}xyz");
            assert_eq!(
                events(stream.as_bytes()),
                [Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)), key('x'), key('y'), key('z'),]
            );
        }
    }

    #[test]
    fn an_interrupted_reply_keeps_the_new_escape_sequence() {
        assert_eq!(events(b"\x1b]4;1;rgb:ff\x1b[A"), [Event::Key(KeyCode::Up.into())]);
    }

    #[test]
    fn escape_and_alt_string_keys_are_released_after_a_short_wait() {
        for bytes in [b"\x1b".as_slice(), b"\x1b]", b"\x1bP", b"\x1b_"] {
            let mut parser = Parser::default();
            let mut received = Vec::new();
            let mut emit = |event| {
                received.push(event);
                true
            };
            parser.feed(bytes, &mut emit);
            parser.since = Instant::now().checked_sub(ESC_WAIT);
            parser.idle(&mut emit);
            assert_eq!(received.len(), 1);
            assert_eq!(parser.pending, b"");
        }
    }

    #[test]
    fn an_unfinished_reply_cannot_hold_input_forever() {
        let mut parser = Parser::default();
        let mut received = Vec::new();
        let mut emit = |event| {
            received.push(event);
            true
        };
        parser.feed(b"\x1b]4;1;rgb:ff", &mut emit);
        parser.since = Instant::now().checked_sub(REPLY_WAIT);
        parser.idle(&mut emit);
        parser.feed(b"z", &mut emit);
        assert_eq!(received, [key('z')]);
    }

    #[rstest]
    #[case::poll_error(false, false)]
    #[case::read_error(true, false)]
    #[case::poll_panic(false, true)]
    #[case::read_panic(true, true)]
    fn an_input_failure_is_retried_with_a_pause(#[case] in_read: bool, #[case] panic: bool) {
        let failed = Cell::new(false);
        let stop = AtomicBool::new(false);
        let mut received = Vec::new();
        let mut pauses = Vec::new();
        let fail = || -> io::Result<bool> {
            assert!(!panic, "input bug");
            Err(io::ErrorKind::Interrupted.into())
        };
        run(
            &stop,
            || if !in_read && !failed.replace(true) { fail() } else { Ok(true) },
            |bytes| {
                if in_read && !failed.replace(true) {
                    return fail().map(|_| 1);
                }
                bytes[0] = b'z';
                Ok(1)
            },
            |event| {
                received.push(event);
                false
            },
            |wait| pauses.push(wait),
        );
        assert_eq!(received, [key('z')]);
        assert_eq!(pauses, [RETRY]);
    }

    #[rstest]
    #[case::eof(None)]
    #[case::hangup(Some(io::Error::from(io::ErrorKind::UnexpectedEof)))]
    #[case::gone(Some(io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error())))]
    fn the_end_of_input_stops_the_reader(#[case] error: Option<io::Error>) {
        let mut error = error;
        run(
            &AtomicBool::new(false),
            || Ok(true),
            |_| error.take().map_or(Ok(0), Err),
            |_| panic!("no event after input ended"),
            |_| panic!("no retry after input ended"),
        );
    }

    #[test]
    fn a_read_error_clears_a_partial_sequence_before_retrying() {
        let mut reads = 0;
        let mut received = Vec::new();
        run(
            &AtomicBool::new(false),
            || Ok(true),
            |bytes| {
                reads += 1;
                match reads {
                    1 => {
                        bytes[..2].copy_from_slice(b"\x1b[");
                        Ok(2)
                    }
                    2 => Err(io::ErrorKind::Interrupted.into()),
                    _ => {
                        bytes[0] = b'z';
                        Ok(1)
                    }
                }
            },
            |event| {
                received.push(event);
                false
            },
            |_| {},
        );
        assert_eq!(received, [key('z')]);
    }

    #[test]
    fn repeated_errors_back_off_until_stopped() {
        let stop = AtomicBool::new(false);
        let mut pauses = 0;
        run(
            &stop,
            || Err(io::Error::other("not ready")),
            |_| panic!("no read"),
            |_| panic!("no event"),
            |wait| {
                assert_eq!(wait, RETRY);
                pauses += 1;
                if pauses == 3 {
                    stop.store(true, Ordering::Relaxed);
                }
            },
        );
        assert_eq!(pauses, 3);
    }

    #[test]
    fn a_hung_up_fd_is_the_end_of_input() {
        let (input, peer) = UnixStream::pair().expect("socket pair");
        drop(peer);
        assert!(readable(&input, Duration::ZERO).is_err_and(|error| ended(&error)));
    }

    #[test]
    fn a_parser_error_drops_only_the_invalid_sequence() {
        let mut parser = Parser::default();
        let mut received = Vec::new();
        parser.feed_with(
            b"xz",
            &mut |event| {
                received.push(event);
                true
            },
            &mut |bytes, more| {
                if bytes == b"x" {
                    return Err(io::Error::other("invalid event"));
                }
                parse::parse_event(bytes, more)
            },
        );
        assert_eq!(received, [key('z')]);
    }
}
