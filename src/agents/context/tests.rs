use super::*;

mod pane {
    use super::*;

    #[test]
    fn the_record_is_the_one_of_the_agent_asked_for() {
        let place = opencode::Place { database: "/d/opencode.db".into(), id: "ses".into() };
        let replaced = Pane {
            codex: Some(codex::Rollout::new("/c/rollout.jsonl".into())),
            opencode: Some(opencode::Session { place: Some(place), ..opencode::Session::default() }),
            ..Pane::default()
        };

        let found = [Known::Opencode, Known::Claude].map(|agent| replaced.record_for(agent));

        assert_eq!(found, [Some(Record::Opencode { database: "/d/opencode.db".into(), session: "ses".into() }), None]);
    }
}

#[expect(unused_qualifications, reason = "preserve the existing test bodies when nesting this module")]
pub(in crate::agents) mod counting_records {
    use super::*;
    use crate::agents::context::counting::tests::{since, sum};
    use crate::test_util::{TempDir, wait_until};

    pub(in crate::agents) const FIXTURE: &str = crate::test_util::fixture!("claude/2.1.296/usage.jsonl");

    #[test]
    fn a_subagent_adds_tokens_through_its_own_incremental_tail() {
        let dir = TempDir::new();
        let cwd = dir.path().join("synthetic");
        let transcript = transcript_path(dir.path(), &cwd, "session").expect("path");
        let sub = transcript.with_extension("").join("subagents/agent-synthetic.jsonl");
        std::fs::create_dir_all(sub.parent().expect("parent")).expect("directory");
        std::fs::write(&transcript, FIXTURE).expect("transcript");
        std::fs::write(&sub, crate::test_util::fixture!("claude/2.1.296/subagent.jsonl")).expect("subagent");
        let session = crate::agents::claude::Session::parse(
            &serde_json::json!({"pid": 1, "sessionId": "session", "cwd": cwd}).to_string(),
            1,
        )
        .expect("session");
        let claude = Claude { pid: 1, args: Vec::new(), session: Some(session) };
        let mut pane = Pane { agent: Some((1, since())), kind: Some(Known::Claude), ..Pane::default() };
        let mut output = 0;
        wait_until("subagent usage", || {
            pane.update(Some(dir.path()), Some(&claude));
            output += sum(pane.take_counted()).1.out;
            output == 100
        });
        pane.update(Some(dir.path()), Some(&claude));
        assert_eq!(sum(pane.take_counted()).1.out, 0);
        let cleared = transcript_path(dir.path(), &cwd, "clear-session").expect("path");
        std::fs::write(cleared, "").expect("clear transcript");
        let session = crate::agents::claude::Session::parse(
            &serde_json::json!({"pid": 1, "sessionId": "clear-session", "cwd": cwd}).to_string(),
            1,
        )
        .expect("clear session");
        pane.update(Some(dir.path()), Some(&Claude { pid: 1, args: Vec::new(), session: Some(session) }));
        pane.update(Some(dir.path()), Some(&claude));
        assert_eq!(sum(pane.take_counted()).1.out, 0);
    }
}
