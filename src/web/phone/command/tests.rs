use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

use rstest::rstest;

use super::*;
use crate::test_util::wait_until;

fn read(bytes: &[u8], limit: usize) -> Vec<Out> {
    let mut seen = Vec::new();
    lines(
        Cursor::new(bytes.to_vec()),
        limit,
        |out| {
            seen.push(out);
            true
        },
        Output::Stdout,
    );
    seen
}

fn line(text: &str) -> Out {
    Out::Line(Output::Stdout(text.into()).json())
}

struct Slow<'a>(&'a [&'a [u8]]);

impl Read for Slow<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Some((first, rest)) = self.0.split_first() else { return Ok(0) };
        buffer[..first.len()].copy_from_slice(first);
        self.0 = rest;
        Ok(first.len())
    }
}

mod reading_lines {
    use super::*;

    #[rstest]
    #[case::splits_on_newline(b"a\nb\n".as_slice(), vec![line("a"), line("b"), Out::End])]
    #[case::keeps_a_carriage_return(b"a\r\nb".as_slice(), vec![line("a\r"), line("b"), Out::End])]
    #[case::sends_the_last_line_without_newline(b"a\nb".as_slice(), vec![line("a"), line("b"), Out::End])]
    #[case::adds_no_empty_line_after_the_last_newline(b"a\n".as_slice(), vec![line("a"), Out::End])]
    #[case::keeps_empty_lines_inside(b"a\n\nb\n".as_slice(), vec![line("a"), line(""), line("b"), Out::End])]
    #[case::nothing(b"".as_slice(), vec![Out::End])]
    #[case::replaces_invalid_utf8(b"a\xffb\n".as_slice(), vec![line("a\u{fffd}b"), Out::End])]
    fn like_the_bridge(#[case] bytes: &[u8], #[case] expected: Vec<Out>) {
        assert_eq!(read(bytes, 100), expected);
    }

    #[test]
    fn a_line_at_the_limit_passes_and_one_more_byte_is_too_long() {
        assert_eq!(read(b"abcd\n", 4), vec![line("abcd"), Out::End]);
        assert_eq!(read(b"x\nabcde\ny\n", 4), vec![line("x"), Out::TooLong, Out::End]);
    }

    #[test]
    fn a_last_line_over_the_limit_is_too_long_too() {
        assert_eq!(read(b"abcde", 4), vec![Out::TooLong, Out::End]);
    }

    #[test]
    fn a_line_split_over_reads_is_one_line() {
        let mut seen = Vec::new();
        lines(
            Slow(&[b"ab", b"c\nd", b"e\n"]),
            10,
            |out| {
                seen.push(out);
                true
            },
            Output::Stderr,
        );

        let stderr = |text: &str| Out::Line(Output::Stderr(text.into()).json());
        assert_eq!(seen, vec![stderr("abc"), stderr("de"), Out::End]);
    }
}

mod the_queue {
    use super::*;

    fn big(size: usize) -> Out {
        Out::Line("x".repeat(size))
    }

    #[test]
    fn a_push_over_the_budget_waits_until_a_pop_makes_room() {
        let queue = Arc::new(Queue::new(10));
        assert!(queue.push(big(8)));
        let waiting = Arc::clone(&queue);
        let pushed = thread::spawn(move || waiting.push(big(8)));

        wait_until("the first line queued", || queue.bytes() == 8);
        thread::sleep(Duration::from_millis(50));
        assert!(!pushed.is_finished());
        assert_eq!(queue.pop(), Some(big(8)));

        assert!(pushed.join().expect("joined"));
        assert_eq!(queue.bytes(), 8);
    }

    #[test]
    fn a_line_bigger_than_the_budget_passes_when_the_queue_is_empty() {
        let queue = Queue::new(4);

        assert!(queue.push(big(100)));
        assert_eq!(queue.pop(), Some(big(100)));
        assert_eq!(queue.bytes(), 0);
    }

    #[test]
    fn closing_wakes_a_waiting_push_with_false() {
        let queue = Arc::new(Queue::new(4));
        assert!(queue.push(big(4)));
        let waiting = Arc::clone(&queue);
        let pushed = thread::spawn(move || waiting.push(big(4)));
        thread::sleep(Duration::from_millis(50));

        queue.close();

        assert!(!pushed.join().expect("joined"));
        assert!(!queue.push(Out::End));
    }
}

#[rstest]
#[case::the_code(Some(3), None, 3)]
#[case::a_signal(None, Some(15), 143)]
#[case::neither(None, None, -1)]
fn the_exit_code(#[case] code: Option<i32>, #[case] signal: Option<i32>, #[case] expected: i32) {
    assert_eq!(exit_code(code, signal), expected);
}

#[test]
fn the_command_runs_without_the_callers_variables_and_reaches_this_server() {
    let socket = PathBuf::from("/tmp/cornercase-test.sock");

    let command = command(Path::new("cornercase"), &["status".into()], &socket);
    let envs: Vec<_> = command.get_envs().collect();

    for variable in [REMOTE_ENV, PANE_ENV, SERVER_ENV, RESTARTED_ENV, TIED_ENV] {
        assert!(envs.iter().any(|(key, value)| *key == variable && value.is_none()), "{variable} is not removed");
    }
    assert!(envs.iter().any(|(key, value)| *key == paths::SOCKET_ENV && *value == Some(socket.as_os_str())));
    assert_eq!(command.get_args().collect::<Vec<_>>(), ["status"]);
}
