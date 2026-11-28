use std::io::Write as _;
use std::path::PathBuf;

use rstest::rstest;

use super::*;
use crate::app::events::{Events, Streamed};
use crate::test_util::TempDir;

struct Transcript {
    _dir: TempDir,
    path: PathBuf,
}

impl Transcript {
    fn new() -> Self {
        let dir = TempDir::new();
        let path = dir.path().join("session.jsonl");
        let transcript = Self { _dir: dir, path };
        transcript.grow();
        transcript
    }

    fn grow(&self) {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&self.path).expect("open");
        writeln!(file, "{{}}").expect("append a line");
    }

    fn pane(&self) -> Vec<(u64, Known, Record)> {
        vec![(1, Known::Claude, Record::Claude(self.path.clone()))]
    }
}

fn said(text: &str, written: &str) -> Said {
    Said { text: text.to_string(), written: Some(written.to_string()) }
}

fn a() -> Said {
    said("I will read the routes first.", "2026-11-27T10:00:01.000Z")
}

fn b() -> Said {
    said("The page is in src/login.rs.", "2026-11-27T10:00:05.000Z")
}

fn reads(due: &[Due]) -> Vec<(u64, u64)> {
    due.iter().map(|due| (due.pane, due.read)).collect()
}

fn read(last_messages: &mut LastMessages, transcript: &Transcript) -> u64 {
    let due = reads(&last_messages.step(transcript.pane()));
    let [(1, read)] = due[..] else { panic!("pane 1 is not read: {due:?}") };
    read
}

fn failed() -> error::Result<Option<Said>> {
    Err(std::io::Error::other("the record went away").into())
}

fn kept_a() -> (LastMessages, Transcript) {
    let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());
    let first = read(&mut last_messages, &transcript);
    assert!(!last_messages.told(1, first, Ok(Some(a()))));
    (last_messages, transcript)
}

fn changed(events: &mut Events, told: bool) -> Vec<u64> {
    assert!(told, "a change");
    let event = Event {
        time: "2026-11-27T10:00:05.000Z".into(),
        what: What::LastMessage { agent: "claude".into() },
        ids: Ids { pane: Some(1), ..Ids::default() },
    };
    events.publish(&[event]);
    let streamed = events.take();
    streamed
        .into_iter()
        .filter(|(_, streamed)| matches!(streamed, Streamed::Event(_)))
        .map(|(client, _)| client)
        .collect()
}

mod last_messages {
    use super::*;

    #[test]
    fn a_pane_found_when_following_starts_keeps_its_message_quiet_and_tells_the_next() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        let first = read(&mut last_messages, &transcript);
        assert!(!last_messages.told(1, first, Ok(Some(a()))));
        transcript.grow();
        let second = read(&mut last_messages, &transcript);
        assert!(last_messages.told(1, second, Ok(Some(b()))));
    }

    #[test]
    fn a_pane_found_on_a_later_step_is_told_its_first_message() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        last_messages.step(Vec::new());
        let first = read(&mut last_messages, &transcript);

        assert!(last_messages.told(1, first, Ok(Some(a()))));
    }

    #[test]
    fn a_pane_found_right_after_a_reader_joined_keeps_its_message_quiet() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        last_messages.step(Vec::new());
        last_messages.unprime();
        let first = read(&mut last_messages, &transcript);

        assert!(!last_messages.told(1, first, Ok(Some(a()))));
    }

    #[rstest]
    #[case::the_same_message(Some(a()), false)]
    #[case::the_same_text_written_later(Some(said("I will read the routes first.", "2026-11-27T10:09:00.000Z")), true)]
    #[case::another_text(Some(said("The page is in src/login.rs.", "2026-11-27T10:00:01.000Z")), true)]
    #[case::no_message_any_more(None, false)]
    fn what_is_told(#[case] now: Option<Said>, #[case] told: bool) {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        let second = read(&mut last_messages, &transcript);

        assert_eq!(last_messages.told(1, second, Ok(now)), told);
    }

    #[test]
    fn a_read_that_failed_keeps_what_was_read_before() {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        let second = read(&mut last_messages, &transcript);

        assert!(!last_messages.told(1, second, failed()));
        transcript.grow();
        let again = read(&mut last_messages, &transcript);
        assert!(!last_messages.told(1, again, Ok(Some(a()))));
    }

    #[test]
    fn an_unchanged_record_that_keeps_failing_is_read_again_three_times_then_waits_for_a_change() {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        let mut tried = read(&mut last_messages, &transcript);

        for _ in 0..RETRIES {
            assert!(!last_messages.told(1, tried, failed()));
            tried = read(&mut last_messages, &transcript);
        }
        assert!(!last_messages.told(1, tried, failed()));
        assert_eq!(reads(&last_messages.step(transcript.pane())), []);

        transcript.grow();
        let changed = read(&mut last_messages, &transcript);
        assert!(!last_messages.told(1, changed, failed()));
        let again = read(&mut last_messages, &transcript);
        assert!(last_messages.told(1, again, Ok(Some(b()))));
    }

    #[test]
    fn a_change_found_while_a_retry_waits_is_retried_as_often_as_a_first_failure() {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        let mut tried = read(&mut last_messages, &transcript);
        for _ in 1..RETRIES {
            assert!(!last_messages.told(1, tried, failed()));
            tried = read(&mut last_messages, &transcript);
        }
        assert!(!last_messages.told(1, tried, failed()));

        transcript.grow();
        let mut tried = read(&mut last_messages, &transcript);
        for _ in 0..RETRIES {
            assert!(!last_messages.told(1, tried, failed()));
            tried = read(&mut last_messages, &transcript);
        }
        assert!(last_messages.told(1, tried, Ok(Some(b()))));
    }

    #[test]
    fn a_record_is_read_once_per_change_and_never_twice_at_once() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        let first = read(&mut last_messages, &transcript);
        assert_eq!(reads(&last_messages.step(transcript.pane())), []);
        transcript.grow();
        assert_eq!(reads(&last_messages.step(transcript.pane())), []);
        last_messages.told(1, first, Ok(Some(a())));
        let second = read(&mut last_messages, &transcript);
        last_messages.told(1, second, Ok(Some(a())));
        assert_eq!(reads(&last_messages.step(transcript.pane())), []);
    }

    #[test]
    fn a_pane_left_out_of_a_step_is_forgotten() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        let first = read(&mut last_messages, &transcript);
        last_messages.step(Vec::new());

        assert!(last_messages.is_empty());
        assert!(!last_messages.told(1, first, Ok(Some(a()))));
    }

    #[test]
    fn what_a_read_from_before_the_pane_was_dropped_finds_changes_nothing() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());
        let stale = read(&mut last_messages, &transcript);
        last_messages.step(Vec::new());
        let current = read(&mut last_messages, &transcript);

        assert!(!last_messages.told(1, stale, Ok(Some(a()))));
        assert_eq!(reads(&last_messages.step(transcript.pane())), [], "the current read still runs");
        assert!(last_messages.told(1, current, Ok(Some(b()))));
    }

    #[test]
    fn what_a_read_from_before_a_clear_finds_changes_nothing() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());
        let stale = read(&mut last_messages, &transcript);
        last_messages.clear();
        last_messages.step(Vec::new());
        let current = read(&mut last_messages, &transcript);

        assert_ne!(current, stale);
        assert!(!last_messages.told(1, stale, Ok(Some(a()))));
        assert!(last_messages.told(1, current, Ok(Some(b()))));
    }
}

mod readers {
    use super::*;

    fn following() -> (Events, Transcript) {
        let mut events = Events::default();
        events.subscribe(1, Vec::new(), false, false);
        let transcript = Transcript::new();
        let first = read(&mut events.last_messages, &transcript);
        assert!(!events.last_messages.told(1, first, Ok(Some(a()))));
        (events, transcript)
    }

    #[test]
    fn a_reader_that_joins_beside_another_is_told_a_change_written_after_it_joined() {
        let (mut events, transcript) = following();
        events.subscribe(2, Vec::new(), false, false);
        transcript.grow();
        let after = read(&mut events.last_messages, &transcript);

        let told = events.last_messages.told(1, after, Ok(Some(b())));

        assert_eq!(changed(&mut events, told), [1, 2]);
    }

    #[test]
    fn a_reader_that_joins_while_a_read_runs_may_be_told_what_it_finds() {
        let (mut events, transcript) = following();
        transcript.grow();
        let running = read(&mut events.last_messages, &transcript);
        events.subscribe(2, Vec::new(), false, false);

        let told = events.last_messages.told(1, running, Ok(Some(b())));

        assert_eq!(changed(&mut events, told), [1, 2]);
    }
}
