use rstest::rstest;

use super::*;
use crate::agents::context::message::tests::{Written, joined_lines};
use crate::test_util::TempDir;

fn said(id: &str, kind: &str, text: &str) -> String {
    let block = serde_json::json!({ "type": kind, kind: text });
    serde_json::json!({
        "type": "assistant",
        "isSidechain": false,
        "timestamp": format!("2026-10-08T19:56:{:02}.241Z", text.len() % 60),
        "message": { "id": id, "model": "claude-opus-5-5", "role": "assistant", "content": [block] },
    })
    .to_string()
}

fn user(text: &str) -> String {
    serde_json::json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
}

fn last_claude(lines: &[String]) -> Option<String> {
    let file = Written::new(&joined_lines(lines));
    claude(&file.path).expect("read the transcript").map(|m| m.text)
}

mod claude {
    use super::*;

    #[test]
    fn the_last_message_is_the_newest_text_of_the_conversation() {
        let found = last_claude(&[
            user("fix it"),
            said("m1", "text", "Looking."),
            said("m1", "tool_use", "{}"),
            user("tool result"),
            said("m2", "thinking", "hmm"),
            said("m2", "text", "Fixed the login form."),
            said("m3", "thinking", "anything else?"),
        ]);

        assert_eq!(found.as_deref(), Some("Fixed the login form."));
    }

    #[test]
    fn the_text_blocks_of_one_message_come_together_in_order() {
        let found = last_claude(&[
            said("m1", "text", "Old."),
            said("m2", "text", "First part."),
            said("m2", "thinking", "hmm"),
            said("m2", "text", "Second part."),
        ]);

        assert_eq!(found.as_deref(), Some("First part.\n\nSecond part."));
    }

    #[test]
    fn it_carries_when_it_was_written() {
        let file = Written::new(&joined_lines(&[said("m1", "text", "Done.")]));

        let found = claude(&file.path).expect("read the transcript");

        assert_eq!(found, Some(Said { text: "Done.".into(), written: Some("2026-10-08T19:56:05.241Z".into()) }));
    }

    #[rstest]
    #[case::a_subagent(
        r#"{"type":"assistant","isSidechain":true,"message":{"id":"s","model":"claude-haiku-4-5","content":[{"type":"text","text":"Subagent."}]}}"#
    )]
    #[case::nothing_to_say(
        r#"{"type":"assistant","message":{"id":"x","model":"<synthetic>","content":[{"type":"text","text":"No response requested."}]},"isApiErrorMessage":false}"#
    )]
    #[case::blank(r#"{"type":"assistant","message":{"id":"b","model":"claude-opus-5-5","content":[{"type":"text","text":"\n\n"}]}}"#)]
    #[case::a_user_quoting_one(r#"{"type":"user","message":{"content":"{\"type\":\"assistant\"}"}}"#)]
    #[case::broken(r#"{"type":"assistant","message":{"id":"z","content":[{"type":"te"#)]
    fn some_lines_are_not_what_it_said(#[case] line: &str) {
        assert_eq!(last_claude(&[said("m1", "text", "Done."), line.to_string()]).as_deref(), Some("Done."));
    }

    #[test]
    fn an_api_error_is_what_it_said_last() {
        let error = r#"{"type":"assistant","message":{"id":"e","model":"<synthetic>","content":[{"type":"text","text":"API Error: 529 Overloaded"}]},"isApiErrorMessage":true}"#;

        assert_eq!(
            last_claude(&[said("m1", "text", "Done."), error.to_string()]).as_deref(),
            Some("API Error: 529 Overloaded")
        );
    }

    #[test]
    fn a_message_older_than_a_mebibyte_of_tool_output_is_found() {
        let output = user(&"x".repeat(1024 * 1024));

        let found = last_claude(&[said("m1", "text", "Read it all."), output.clone(), output]);

        assert_eq!(found.as_deref(), Some("Read it all."));
    }

    #[test]
    fn a_transcript_not_written_yet_has_no_message() {
        let dir = TempDir::new();

        assert_eq!(claude(&dir.path().join("none.jsonl")).expect("no error"), None);
    }

    #[test]
    fn a_transcript_without_a_reply_has_no_message() {
        assert_eq!(last_claude(&[user("hello"), said("m1", "thinking", "hmm")]), None);
    }

    #[test]
    fn a_transcript_that_cannot_be_read_says_why() {
        let dir = TempDir::new();

        let error = claude(dir.path()).expect_err("a folder is no transcript");

        assert!(error.to_string().starts_with(&format!("cannot read `{}`", dir.path().display())), "{error}");
    }
}

mod conversation {
    use super::*;

    fn heard(text: &str) -> Vec<(Role, String)> {
        let file = Written::new(text);
        let mut heard = Vec::new();
        super::super::said(&file.path, &mut |spoken| {
            heard.push((spoken.role, spoken.said.text));
            true
        })
        .expect("read the transcript");
        heard
    }

    #[test]
    fn gives_prompts_and_replies_newest_first_without_tool_results() {
        let human = serde_json::json!({
            "type": "user", "origin": { "kind": "human" }, "message": { "content": "fix it" }
        })
        .to_string();
        let result = serde_json::json!({
            "type": "user", "message": { "content": [{ "type": "tool_result", "content": "ok" }] }
        })
        .to_string();
        let heard =
            heard(&joined_lines(&[human, said("m1", "text", "Looking."), result, said("m2", "text", "Fixed.")]));

        assert_eq!(
            heard,
            [(Role::Reply, "Fixed.".into()), (Role::Reply, "Looking.".into()), (Role::Prompt, "fix it".into())]
        );
    }

    #[test]
    fn counts_the_human_prompts_of_the_queue_once_each() {
        let heard = heard(crate::test_util::fixture!("claude/2.1.296/queued-prompts.jsonl"));

        assert_eq!(
            heard,
            [(Role::Prompt, "synthetic dequeued prompt".into()), (Role::Prompt, "synthetic absorbed prompt".into())]
        );
    }

    #[test]
    fn leaves_out_sidechains_and_meta_prompts() {
        let meta = serde_json::json!({
            "type": "user", "origin": { "kind": "human" }, "isMeta": true, "message": { "content": "caveat" }
        })
        .to_string();
        let side = serde_json::json!({
            "type": "user", "origin": { "kind": "human" }, "isSidechain": true, "message": { "content": "agent" }
        })
        .to_string();

        assert_eq!(heard(&joined_lines(&[meta, side])), []);
    }

    #[test]
    fn stops_when_asked() {
        let file = Written::new(&joined_lines(&[said("m1", "text", "one"), said("m2", "text", "two")]));
        let mut seen = 0;
        super::super::said(&file.path, &mut |_| {
            seen += 1;
            false
        })
        .expect("read the transcript");

        assert_eq!(seen, 1);
    }
}
