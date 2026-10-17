use std::sync::mpsc;

use super::*;
use crate::agents::context;
use crate::control::Response;
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
            stage: Stage::Removing { force: false, unlock: false },
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

mod checks {
    use super::*;

    #[test]
    fn a_removal_check_goes_with_its_client() {
        let dir = TempDir::new();
        let (tx, _rx) = mpsc::channel();
        let mut app = App::new("/bin/sh".into(), HostTheme::default(), dir.path().join("config.json"), tx);
        let checking = Pending { client: Some(1), key: 1, timeout: None, done: Done::default(), stage: Stage::Check };
        app.requests.pending.push(checking);
        app.requests.open.push(1);

        app.forget(1);

        assert_eq!(app.requests.pending.len(), 0);
    }
}

mod unavailable_operations {
    use super::*;
    use crate::control::contract::{self as wire_contract, Code, Refusal};

    const OFF: &[(&str, &str)] = &[("notify", "no notifications on this machine")];

    #[rstest::rstest]
    #[case::opted_in(true)]
    #[case::legacy(false)]
    fn an_operation_listed_as_off_is_unavailable(#[case] structured: bool) {
        let value = serde_json::json!({"command": "notify", "args": {"text": "hi"}});

        let refusal = contract::read(&value, structured, OFF).expect_err("refused");

        assert_eq!(
            refusal,
            Refusal::new(
                Code::Unavailable,
                "`notify` is not available on this server: no notifications on this machine"
            )
        );
    }

    #[test]
    fn the_descriptor_lists_the_same_operations_as_off() {
        let described = wire_contract::describe("1-1", OFF);

        let off: Vec<_> = described.operations.iter().filter(|op| !op.available).map(|op| op.name.as_str()).collect();

        assert_eq!(off, ["notify"]);
    }

    #[test]
    fn this_server_turns_no_operation_off() {
        assert!(contract::descriptor().operations.iter().all(|op| op.available));
    }
}
