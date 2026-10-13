use std::sync::mpsc;

use super::*;
use crate::agents::context;
use crate::host::theme::HostTheme;
use crate::test_util::TempDir;

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
