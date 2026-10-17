use std::cell::Cell;
use std::os::unix::net::UnixStream;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use rstest::rstest;

use super::*;

impl PartialEq<Event> for Input {
    fn eq(&self, other: &Event) -> bool {
        matches!(self, Self::Event(event) if event == other)
    }
}

fn activated(token: &str) -> Input {
    Input::Activated(token.into())
}

fn events(bytes: &[u8]) -> Vec<Input> {
    let mut events = Vec::new();
    Parser::default().feed(bytes, false, &mut |event| {
        events.push(event);
        true
    });
    events
}

fn key(c: char) -> Event {
    Event::Key(KeyCode::Char(c).into())
}

#[rstest]
#[case::sixel(b"\x1b[?1;0;256S", None)]
#[case::geometry(b"\x1b[?2;0;1000;570S", None)]
#[case::attributes(b"\x1b[?62;22;52c", None)]
#[case::secondary_attributes(b"\x1b[>1;400;0c", None)]
#[case::cell_size(b"\x1b[6;34;16t", None)]
#[case::color(b"\x1b]4;255;rgb:1111/2222/3333\x1b\\", None)]
#[case::background(b"\x1b]11;rgb:00/00/00\x07", None)]
#[case::terminal(b"\x1bP>|ghostty 1.3.1\x1b\\", None)]
#[case::kitty(b"\x1b_Gi=31;OK\x1b\\", None)]
#[case::clicked(b"\x1b]99;i=x;\x1b\\", Some("x"))]
#[case::button(b"\x1b]99;i=x;2\x1b\\", Some("x"))]
#[case::bell(b"\x1b]99;i=x;\x07", Some("x"))]
#[case::contour(b"\x1b]99;i=x:p=activated;\x1b\\", Some("x"))]
#[case::closed(b"\x1b]99;i=x:p=close;\x1b\\", None)]
#[case::alive(b"\x1b]99;i=q:p=alive;x,y\x1b\\", None)]
#[case::query(b"\x1b]99;i=q1:p=?;a=focus,report:o=always,unfocused:c=1\x1b\\", None)]
#[case::bad_id(b"\x1b]99;i=x y;\x1b\\", None)]
fn late_replies_leave_the_next_click_and_key_alone(#[case] reply: &[u8], #[case] token: Option<&str>) {
    let click = b"\x1b[<0;8;5M";
    let stream = [reply, click, b"z"].concat();
    let mut expected: Vec<Input> = token.map(activated).into_iter().collect();
    expected.extend([
        Input::Event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7,
            row: 4,
            modifiers: KeyModifiers::NONE,
        })),
        Input::Event(key('z')),
    ]);
    assert_eq!(events(&stream), expected);
    for at in 1..stream.len() {
        let mut parser = Parser::new(true);
        let mut received = Vec::new();
        let mut emit = |event| {
            received.push(event);
            true
        };
        parser.feed(&stream[..at], false, &mut emit);
        parser.feed(&stream[at..], false, &mut emit);
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
        let mut parser = Parser::new(true);
        let mut received = Vec::new();
        for chunk in bytes.chunks(size) {
            parser.feed(chunk, false, &mut |event| {
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
        false,
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
    let text = "hello \x1b]4;1;rgb:ff/00/00\x07 \x1b[?1;0;256S \x1b]99;i=x;\x1b\\";
    let stream = format!("\x1b[200~{text}\x1b[201~");
    assert_eq!(events(stream.as_bytes()), [Event::Paste(text.into())]);
}

#[test]
fn a_click_report_split_after_its_prefix_is_still_read() {
    let stream = b"\x1b]99;i=x;\x1b\\z";
    for at in 5..stream.len() {
        let mut parser = Parser::default();
        let mut received = Vec::new();
        let mut emit = |input| {
            received.push(input);
            true
        };
        parser.feed(&stream[..at], false, &mut emit);
        parser.feed(&stream[at..], false, &mut emit);
        assert_eq!(received, [activated("x"), Input::Event(key('z'))], "split at {at}");
    }
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

#[rstest]
#[case::escape(b"\x1b", KeyCode::Esc.into())]
#[case::alt_bracket(b"\x1b]", KeyEvent::new(KeyCode::Char(']'), KeyModifiers::ALT))]
#[case::alt_p(b"\x1bP", KeyEvent::new(KeyCode::Char('P'), KeyModifiers::ALT))]
#[case::alt_underscore(b"\x1b_", KeyEvent::new(KeyCode::Char('_'), KeyModifiers::ALT))]
fn escape_keys_arrive_immediately_when_no_reply_is_expected(#[case] bytes: &[u8], #[case] expected: KeyEvent) {
    assert_eq!(events(bytes), [Event::Key(expected)]);
    let mut parser = Parser::new(true);
    parser.replies_until = Some(Instant::now());
    let mut received = Vec::new();
    parser.feed(bytes, false, &mut |event| {
        received.push(event);
        true
    });
    assert_eq!(received, [Event::Key(expected)]);
}

#[rstest]
#[case::separate_reads(false, false, 2)]
#[case::one_read(false, true, 1)]
#[case::separate_reads_with_late_replies(true, false, 2)]
#[case::one_read_with_late_replies(true, true, 1)]
fn consecutive_escapes_keep_the_original_read_boundaries(
    #[case] late: bool,
    #[case] together: bool,
    #[case] count: usize,
) {
    let mut parser = Parser::new(late);
    let mut received = Vec::new();
    let mut emit = |event| {
        received.push(event);
        true
    };
    if together {
        parser.feed(b"\x1b\x1b", false, &mut emit);
    } else {
        parser.feed(b"\x1b", false, &mut emit);
        parser.feed(b"\x1b", false, &mut emit);
    }
    parser.since = Instant::now().checked_sub(ESC_WAIT);
    parser.idle(&mut emit);
    assert_eq!(received, vec![Event::Key(KeyCode::Esc.into()); count]);
}

#[rstest]
#[case::escape(b"\x1b")]
#[case::alt_bracket(b"\x1b]")]
#[case::alt_p(b"\x1bP")]
#[case::alt_underscore(b"\x1b_")]
fn a_held_key_bounds_the_next_poll_by_its_remaining_wait(#[case] bytes: &[u8]) {
    let mut parser = Parser::new(true);
    parser.feed(bytes, false, &mut |_| panic!("key is still held"));
    let elapsed = ESC_WAIT * 3 / 4;
    parser.since = Instant::now().checked_sub(elapsed);
    assert!(parser.wait() <= ESC_WAIT.saturating_sub(elapsed));
    parser.since = Instant::now().checked_sub(ESC_WAIT);
    assert_eq!(parser.wait(), Duration::ZERO);
}

#[test]
fn the_input_loop_uses_the_held_keys_remaining_wait() {
    let stop = AtomicBool::new(false);
    let mut polls = 0;
    run(
        &stop,
        true,
        |wait| {
            polls += 1;
            if polls == 1 {
                assert_eq!(wait, super::super::INPUT_POLL);
                Ok(true)
            } else {
                assert!(wait < ESC_WAIT);
                stop.store(true, Ordering::Relaxed);
                Ok(false)
            }
        },
        |bytes| {
            bytes[0] = 0x1b;
            Ok(1)
        },
        |_| true,
        |_| panic!("poll must use the remaining hold time"),
    );
    assert_eq!(polls, 2);
}

#[test]
fn a_full_read_can_keep_a_trailing_escape_for_the_next_read() {
    let mut parser = Parser::default();
    let mut received = Vec::new();
    let mut emit = |event| {
        received.push(event);
        true
    };
    parser.feed(b"\x1b", true, &mut emit);
    parser.feed(b"[A", false, &mut emit);
    assert_eq!(received, [Event::Key(KeyCode::Up.into())]);
}

#[test]
fn dropping_a_reply_opens_a_brief_window_for_the_next_reply() {
    let mut parser = Parser::default();
    let mut received = Vec::new();
    let mut emit = |event| {
        received.push(event);
        true
    };
    parser.feed(b"\x1b[?1;0;256S", false, &mut emit);
    parser.feed(b"\x1b", false, &mut emit);
    parser.feed(b"]4;1;rgb:00/00/00\x07z", false, &mut emit);
    assert_eq!(received, [key('z')]);
}

#[test]
fn an_interrupted_reply_keeps_the_new_escape_sequence() {
    assert_eq!(events(b"\x1b]4;1;rgb:ff\x1b[A"), [Event::Key(KeyCode::Up.into())]);
}

#[test]
fn escape_and_alt_string_keys_are_released_after_a_short_wait() {
    for bytes in [b"\x1b".as_slice(), b"\x1b]", b"\x1bP", b"\x1b_"] {
        let mut parser = Parser::new(true);
        let mut received = Vec::new();
        let mut emit = |event| {
            received.push(event);
            true
        };
        parser.feed(bytes, false, &mut emit);
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
    parser.feed(b"\x1b]4;1;rgb:ff", false, &mut emit);
    parser.since = Instant::now().checked_sub(REPLY_WAIT);
    parser.idle(&mut emit);
    parser.feed(b"z", false, &mut emit);
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
        Err(io::Error::other("input failure"))
    };
    run(
        &stop,
        false,
        |_| if !in_read && !failed.replace(true) { fail() } else { Ok(true) },
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
        false,
        |_| Ok(true),
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
        false,
        |_| Ok(true),
        |bytes| {
            reads += 1;
            match reads {
                1 => {
                    bytes[..2].copy_from_slice(b"\x1b[");
                    Ok(2)
                }
                2 => Err(io::Error::other("input failure")),
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

#[rstest]
#[case::poll(false)]
#[case::read(true)]
fn an_interrupted_system_call_keeps_a_partial_mouse_sequence(#[case] in_read: bool) {
    let polls = Cell::new(0);
    let mut reads = 0;
    let mut received = Vec::new();
    run(
        &AtomicBool::new(false),
        false,
        |_| {
            polls.set(polls.get() + 1);
            if !in_read && polls.get() == 2 { Err(io::ErrorKind::Interrupted.into()) } else { Ok(true) }
        },
        |bytes| {
            reads += 1;
            if in_read && reads == 2 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let chunk = if reads == 1 { b"\x1b[<0;8;".as_slice() } else { b"5M" };
            bytes[..chunk.len()].copy_from_slice(chunk);
            Ok(chunk.len())
        },
        |event| {
            received.push(event);
            false
        },
        |_| panic!("an interrupted call must retry without pausing"),
    );
    assert_eq!(
        received,
        [Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7,
            row: 4,
            modifiers: KeyModifiers::NONE,
        })]
    );
}

#[test]
fn repeated_errors_back_off_until_stopped() {
    let stop = AtomicBool::new(false);
    let mut pauses = 0;
    run(
        &stop,
        false,
        |_| Err(io::Error::other("not ready")),
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
        false,
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
