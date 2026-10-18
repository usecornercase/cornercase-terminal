use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rstest::rstest;

use super::*;

const PID: Option<i32> = Some(42);

fn press(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

fn typed(text: &str, now: Instant) -> Typing {
    let mut typing = Typing::default();
    for c in text.chars() {
        typing.key(press(KeyCode::Char(c), KeyModifiers::NONE), PID, now);
    }
    typing
}

mod a_draft {
    use super::*;

    #[test]
    fn starts_with_a_character() {
        let now = Instant::now();
        let typing = typed("fix", now);
        assert!(typing.draft(PID, now));
        assert!(typing.recent(now, Duration::from_secs(2)));
    }

    #[rstest]
    #[case::enter(press(KeyCode::Enter, KeyModifiers::NONE))]
    #[case::esc(press(KeyCode::Esc, KeyModifiers::NONE))]
    #[case::ctrl_c(press(KeyCode::Char('c'), KeyModifiers::CONTROL))]
    #[case::ctrl_u(press(KeyCode::Char('u'), KeyModifiers::CONTROL))]
    fn ends_with_a_key_that_sends_or_drops_the_line(#[case] key: KeyEvent) {
        let now = Instant::now();
        let mut typing = typed("fix", now);
        typing.key(key, PID, now);
        assert!(!typing.draft(PID, now));
        assert!(typing.recent(now, Duration::from_secs(2)), "the key still counts as typing");
    }

    #[rstest]
    #[case::shift_enter(press(KeyCode::Enter, KeyModifiers::SHIFT))]
    #[case::backspace(press(KeyCode::Backspace, KeyModifiers::NONE))]
    #[case::left(press(KeyCode::Left, KeyModifiers::NONE))]
    fn stays_through_editing_keys(#[case] key: KeyEvent) {
        let now = Instant::now();
        let mut typing = typed("fix", now);
        typing.key(key, PID, now);
        assert!(typing.draft(PID, now));
    }

    #[test]
    fn is_not_started_by_a_control_key_or_a_release() {
        let now = Instant::now();
        let mut typing = Typing::default();
        typing.key(press(KeyCode::Char('l'), KeyModifiers::CONTROL), PID, now);
        let mut release = press(KeyCode::Char('x'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        typing.key(release, PID, now);
        assert!(!typing.draft(PID, now));
    }

    #[test]
    fn belongs_to_the_program_it_was_typed_into() {
        let now = Instant::now();
        let typing = typed("fix", now);
        assert!(!typing.draft(Some(7), now));
    }

    #[test]
    fn lapses_after_a_minute_without_typing() {
        let now = Instant::now();
        let typing = typed("fix", now);
        assert!(typing.draft(PID, now + DRAFT_FOR.saturating_sub(Duration::from_millis(1))));
        assert!(!typing.draft(PID, now + DRAFT_FOR));
    }

    #[rstest]
    #[case::text("a note", true)]
    #[case::line("ls\r", false)]
    #[case::lines("one\ntwo\n", false)]
    #[case::empty("", false)]
    fn follows_a_paste(#[case] text: &str, #[case] draft: bool) {
        let now = Instant::now();
        let mut typing = Typing::default();
        typing.paste(text, PID, now);
        assert_eq!(typing.draft(PID, now), draft);
    }
}
