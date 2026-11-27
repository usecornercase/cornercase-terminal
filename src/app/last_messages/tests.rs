use std::io::Write as _;
use std::path::PathBuf;

use rstest::rstest;

use super::*;
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

fn panes(due: &[(u64, Known, Record)]) -> Vec<u64> {
    due.iter().map(|(pane, ..)| *pane).collect()
}

fn failed() -> error::Result<Option<Said>> {
    Err(std::io::Error::other("the record went away").into())
}

fn kept_a() -> (LastMessages, Transcript) {
    let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());
    last_messages.step(transcript.pane());
    assert!(!last_messages.told(1, Ok(Some(a()))));
    (last_messages, transcript)
}

mod last_messages {
    use super::*;

    #[test]
    fn a_pane_found_when_following_starts_keeps_its_message_quiet_and_tells_the_next() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        assert_eq!(panes(&last_messages.step(transcript.pane())), [1]);
        assert!(!last_messages.told(1, Ok(Some(a()))));
        transcript.grow();
        assert_eq!(panes(&last_messages.step(transcript.pane())), [1]);
        assert!(last_messages.told(1, Ok(Some(said("The page is in src/login.rs.", "2026-11-27T10:00:05.000Z")))));
    }

    #[test]
    fn a_pane_found_on_a_later_step_is_told_its_first_message() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        last_messages.step(Vec::new());
        last_messages.step(transcript.pane());

        assert!(last_messages.told(1, Ok(Some(a()))));
    }

    #[test]
    fn a_pane_found_right_after_a_reader_joined_keeps_its_message_quiet() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        last_messages.step(Vec::new());
        last_messages.unprime();
        last_messages.step(transcript.pane());

        assert!(!last_messages.told(1, Ok(Some(a()))));
    }

    #[rstest]
    #[case::the_same_message(Some(a()), false)]
    #[case::the_same_text_written_later(Some(said("I will read the routes first.", "2026-11-27T10:09:00.000Z")), true)]
    #[case::another_text(Some(said("The page is in src/login.rs.", "2026-11-27T10:00:01.000Z")), true)]
    #[case::no_message_any_more(None, false)]
    fn what_is_told(#[case] now: Option<Said>, #[case] told: bool) {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        last_messages.step(transcript.pane());

        assert_eq!(last_messages.told(1, Ok(now)), told);
    }

    #[test]
    fn a_read_that_failed_keeps_what_was_read_before() {
        let (mut last_messages, transcript) = kept_a();
        transcript.grow();
        last_messages.step(transcript.pane());

        assert!(!last_messages.told(1, failed()));
        transcript.grow();
        assert_eq!(panes(&last_messages.step(transcript.pane())), [1]);
        assert!(!last_messages.told(1, Ok(Some(a()))));
    }

    #[test]
    fn a_record_is_read_once_per_change_and_never_twice_at_once() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        assert_eq!(panes(&last_messages.step(transcript.pane())), [1]);
        assert_eq!(panes(&last_messages.step(transcript.pane())), Vec::<u64>::new());
        transcript.grow();
        assert_eq!(panes(&last_messages.step(transcript.pane())), Vec::<u64>::new());
        last_messages.told(1, Ok(Some(a())));
        assert_eq!(panes(&last_messages.step(transcript.pane())), [1]);
        last_messages.told(1, Ok(Some(a())));
        assert_eq!(panes(&last_messages.step(transcript.pane())), Vec::<u64>::new());
    }

    #[test]
    fn a_pane_left_out_of_a_step_is_forgotten() {
        let (mut last_messages, transcript) = (LastMessages::default(), Transcript::new());

        last_messages.step(transcript.pane());
        last_messages.step(Vec::new());

        assert!(last_messages.is_empty());
        assert!(!last_messages.told(1, Ok(Some(a()))));
    }
}
