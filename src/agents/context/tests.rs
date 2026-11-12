use super::*;

pub(in crate::agents) fn pane_look(pid: i32) -> Look<'static> {
    Look {
        pid,
        args: Vec::new(),
        title: String::new(),
        screen: &String::new,
        claude_dir: None,
        vibe_dir: None,
        account_dir: None,
    }
}

mod pane {
    use super::*;
    use crate::agents::claude::transcript::transcript_path;
    use crate::test_util::TempDir;

    #[test]
    fn the_record_is_the_one_of_the_agent_asked_for() {
        let dir = TempDir::new();
        let path = transcript_path(dir.path(), dir.path(), "session").expect("path");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(&path, "").expect("transcript");
        std::fs::create_dir(dir.path().join("sessions")).expect("sessions");
        std::fs::write(
            dir.path().join("sessions/1.json"),
            serde_json::json!({"pid": 1, "sessionId": "session", "cwd": dir.path()}).to_string(),
        )
        .expect("session");
        let mut replaced = Pane::default();
        replaced.update(Some(Known::Claude), &Look { claude_dir: Some(dir.path()), ..pane_look(1) });

        let found = Known::ALL.map(|agent| replaced.record_for(agent));

        assert_eq!(found, [Some(Record::Claude(path)), None, None, None, None, None, None, None]);
    }
}

mod subagents {
    use super::*;
    use crate::agents::claude::transcript::transcript_path;
    use crate::test_util::{TempDir, wait_until};

    const RUNNING: &str =
        r#"{"type":"user","isSidechain":true,"agentId":"a1","message":{"role":"user","content":"look"}}"#;
    const DONE: &str = r#"{"type":"assistant","isSidechain":true,"agentId":"a1","timestamp":"2026-10-17T10:00:00.000Z","message":{"id":"m","model":"synthetic","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}}"#;

    struct Setup {
        dir: TempDir,
        sub: PathBuf,
        pane: Pane,
    }

    impl Setup {
        fn new(status: &str) -> Self {
            let dir = TempDir::new();
            let transcript = transcript_path(dir.path(), dir.path(), "session").expect("path");
            let sub = transcript.with_extension("").join("subagents/agent-a1.jsonl");
            std::fs::create_dir_all(sub.parent().expect("parent")).expect("directory");
            std::fs::write(&transcript, "").expect("transcript");
            std::fs::write(sub.with_extension("meta.json"), r#"{"requestShape":"background"}"#).expect("meta");
            std::fs::write(&sub, format!("{RUNNING}\n")).expect("subagent");
            std::fs::create_dir(dir.path().join("sessions")).expect("sessions");
            let setup = Self { dir, sub, pane: Pane::watching(1, Known::Claude, SystemTime::UNIX_EPOCH) };
            setup.status(status);
            setup
        }

        fn status(&self, status: &str) {
            std::fs::write(
                self.dir.path().join("sessions/1.json"),
                serde_json::json!({"pid": 1, "sessionId": "session", "cwd": self.dir.path(), "status": status})
                    .to_string(),
            )
            .expect("session");
        }

        fn settle(&mut self, activity: Activity, subagents: usize) {
            let look = Look { claude_dir: Some(self.dir.path()), ..pane_look(1) };
            wait_until("the subagents are read", || {
                self.pane.update(Some(Known::Claude), &look);
                self.pane.activity("") == Some(activity) && self.pane.subagents() == subagents
            });
        }
    }

    #[test]
    fn an_idle_claude_with_a_running_subagent_keeps_working() {
        let mut setup = Setup::new("idle");
        setup.settle(Activity::Subagents, 1);

        std::fs::write(&setup.sub, format!("{RUNNING}\n{DONE}\n")).expect("final reply");

        setup.settle(Activity::Idle, 0);
    }

    #[test]
    fn a_background_shell_left_with_a_subagent_waits_on_the_subagent() {
        Setup::new("shell").settle(Activity::Subagents, 1);
    }

    #[test]
    fn a_question_wins_over_subagents() {
        Setup::new("waiting").settle(Activity::Waiting, 1);
    }

    #[test]
    fn a_busy_agent_stays_working_and_counts_them() {
        Setup::new("busy").settle(Activity::Working, 1);
    }
}

pub(in crate::agents) mod counting_records {
    use super::*;
    use crate::agents::claude::transcript::transcript_path;
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
        std::fs::create_dir(dir.path().join("sessions")).expect("sessions");
        std::fs::write(
            dir.path().join("sessions/1.json"),
            serde_json::json!({"pid": 1, "sessionId": "session", "cwd": cwd}).to_string(),
        )
        .expect("session");
        let look = Look { claude_dir: Some(dir.path()), ..pane_look(1) };
        let mut pane = Pane { agent: Some((1, since())), kind: Some(Known::Claude), ..Pane::default() };
        let mut output = 0;
        wait_until("subagent usage", || {
            pane.update(Some(Known::Claude), &look);
            output += sum(pane.take_counted()).1.out;
            output == 100
        });
        pane.update(Some(Known::Claude), &look);
        assert_eq!(sum(pane.take_counted()).1.out, 0);
        let cleared = transcript_path(dir.path(), &cwd, "clear-session").expect("path");
        std::fs::write(cleared, "").expect("clear transcript");
        std::fs::write(
            dir.path().join("sessions/1.json"),
            serde_json::json!({"pid": 1, "sessionId": "clear-session", "cwd": cwd}).to_string(),
        )
        .expect("clear session");
        pane.update(Some(Known::Claude), &look);
        std::fs::write(
            dir.path().join("sessions/1.json"),
            serde_json::json!({"pid": 1, "sessionId": "session", "cwd": cwd}).to_string(),
        )
        .expect("session");
        pane.update(Some(Known::Claude), &look);
        assert_eq!(sum(pane.take_counted()).1.out, 0);
    }
}

mod order {
    use serde_json::json;

    use super::*;
    use crate::agents::vibe::fake::FakeVibe;
    use crate::test_util::TempDir;

    fn heard(record: &Record) -> Vec<String> {
        let mut texts = Vec::new();
        record
            .said(&mut |spoken| {
                texts.push(spoken.said.text);
                true
            })
            .expect("read the record");
        texts
    }

    #[test]
    fn said_comes_newest_first_except_from_vibe() {
        let dir = TempDir::new();
        let reply = |id: &str, text: &str| {
            json!({
                "type": "assistant", "isSidechain": false, "timestamp": "2026-10-08T19:56:01.000Z",
                "message": {"id": id, "model": "claude-opus-5-5", "role": "assistant",
                            "content": [{"type": "text", "text": text}]},
            })
            .to_string()
        };
        let path = dir.path().join("claude.jsonl");
        std::fs::write(&path, format!("{}\n{}\n", reply("m1", "older"), reply("m2", "newer"))).expect("transcript");
        let fake = FakeVibe::new();
        let part = |text: &str| json!({"type": "action_result", "payload": {"result": {"result": {"parts": [{"type": "text", "text": text}]}}}});
        fake.journal(&[part("older"), part("newer")]);

        let claude = Record::Claude(path);
        let vibe = Record::Vibe(fake.session());

        assert_eq!(
            (claude.said_newest_first(), heard(&claude)),
            (true, vec!["newer".to_string(), "older".to_string()])
        );
        assert_eq!((vibe.said_newest_first(), heard(&vibe)), (false, vec!["older".to_string(), "newer".to_string()]));
    }
}
