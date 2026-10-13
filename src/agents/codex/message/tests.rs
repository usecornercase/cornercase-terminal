use super::*;
use crate::agents::context::message::tests::Written;

mod codex {
    use super::*;

    const CONTEXT: &str = crate::test_util::fixture!("codex/0.160.0/context.jsonl");
    const REPLY: &str = crate::test_util::fixture!("codex/0.160.0/reply.jsonl");

    fn last_codex(text: &str) -> Option<Said> {
        codex(&Written::new(text).path).expect("read the rollout")
    }

    #[test]
    fn the_last_message_is_the_newest_reply_of_the_thread() {
        let found = last_codex(&format!("{CONTEXT}{REPLY}"));

        assert_eq!(
            found,
            Some(Said {
                text: "The login form now checks the password.\n\nTests pass.".into(),
                written: Some("2026-10-05T12:00:37.000Z".into())
            })
        );
    }

    #[test]
    fn a_progress_note_counts_until_a_newer_reply() {
        let note = REPLY.lines().find(|line| line.contains("commentary")).expect("a progress note");

        assert_eq!(
            last_codex(&format!("{CONTEXT}{note}\n")).map(|m| m.text).as_deref(),
            Some("Checking the form first.")
        );
    }

    #[test]
    fn a_rollout_without_a_reply_has_no_message() {
        let user = REPLY.lines().find(|line| line.contains(r#""role":"user""#)).expect("a user message");

        assert_eq!(last_codex(&format!("{CONTEXT}{user}\n")), None);
    }
}
