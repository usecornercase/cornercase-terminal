use std::sync::mpsc;

use super::*;
use crate::host_theme::HostTheme;
use crate::test_util::TempDir;

mod shell_quote {
    use rstest::rstest;

    use super::super::shell_quote;

    #[rstest]
    #[case::plain("/home/me/wt-1.x", "/home/me/wt-1.x")]
    #[case::a_space("/home/me/my wt", "'/home/me/my wt'")]
    #[case::a_single_quote("/home/it's", "'/home/it'\\''s'")]
    fn paths_are_quoted_for_a_shell(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(shell_quote(path), expected);
    }
}

mod holds_prompts {
    use rstest::rstest;

    use super::super::holds_prompts;
    use crate::activity::Status;
    use crate::agents::{CLAUDE, CODEX, GEMINI, OPENCODE};

    #[rstest]
    #[case::codex_at_work(Some(CODEX), Some(Status::Working), true)]
    #[case::codex_asking(Some(CODEX), Some(Status::Waiting), true)]
    #[case::gemini_at_work(Some(GEMINI), Some(Status::Working), true)]
    #[case::gemini_asking(Some(GEMINI), Some(Status::Waiting), true)]
    #[case::gemini_idle(Some(GEMINI), Some(Status::Idle), false)]
    #[case::codex_idle(Some(CODEX), Some(Status::Idle), false)]
    #[case::claude_at_work(Some(CLAUDE), Some(Status::Working), false)]
    #[case::opencode_at_work(Some(OPENCODE), Some(Status::Working), false)]
    fn agents_that_queue_prompts_record_them_late(
        #[case] agent: Option<&str>,
        #[case] status: Option<Status>,
        #[case] holds: bool,
    ) {
        assert_eq!(holds_prompts(agent, status), holds);
    }
}

mod answer_lost_requests {
    use super::*;

    #[test]
    fn answers_the_bug_to_each_request_with_nothing_left_pending() {
        let dir = TempDir::new();
        let (tx, _rx) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), tx);
        let removing = Pending {
            client: Some(1),
            key: 1,
            timeout: None,
            done: Done::default(),
            stage: Stage::Removing { force: false },
        };
        app.requests.pending.push(removing);
        app.requests.open.extend([1, 2]);

        app.answer_lost_requests();

        let bug = serde_json::to_string(&Response::Error(error::Error::Bug.to_string())).expect("json");
        assert_eq!(app.take_answers(), [(2, bug)]);
    }
}

mod forget {
    use super::*;

    #[test]
    fn a_last_message_read_goes_with_its_client() {
        let dir = TempDir::new();
        let (tx, _rx) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), tx);
        let reading = Pending { client: Some(1), key: 1, timeout: None, done: pane_ids(4), stage: Stage::Reading };
        app.requests.pending.push(reading);
        app.requests.open.push(1);

        app.forget(1);
        let left = app.requests.pending.len();
        app.message_read(1, Ok(Some(context::Said { text: "Done.".into(), written: None })));

        assert_eq!((left, app.take_answers()), (0, Vec::new()));
    }
}
