use super::*;

#[test]
fn a_server_without_restart_when_idle_is_called_too_old_to_wait() {
    let refused = Error::Control(control::unknown_command("restart-when-idle"));

    assert_eq!(too_old_to_wait(refused).to_string(), TOO_OLD_TO_WAIT);
}

mod kill_server {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::outside(false, false, false, Kill::Go)]
    #[case::outside_with_terminal(false, false, true, Kill::Go)]
    #[case::inside_with_yes(true, true, false, Kill::Go)]
    #[case::inside_at_a_terminal(true, false, true, Kill::Ask)]
    #[case::inside_without_terminal(true, false, false, Kill::Refuse)]
    fn asks_only_from_inside(#[case] inside: bool, #[case] yes: bool, #[case] terminal: bool, #[case] expected: Kill) {
        assert_eq!(decide(inside, yes, terminal), expected);
    }
}

#[cfg(unix)]
mod a_tied_wait {
    use std::io::{self, Write};
    use std::os::fd::{AsFd, OwnedFd};
    use std::sync::mpsc;
    use std::thread;

    use rstest::rstest;

    use super::*;
    use crate::control::Response;

    const WAIT: Duration = Duration::from_secs(10);

    struct Input {
        fd: OwnedFd,
        keep: Box<dyn std::any::Any + Send>,
    }

    fn pipe() -> Input {
        let (reader, writer) = io::pipe().expect("a pipe");
        Input { fd: reader.into(), keep: Box::new(writer) }
    }

    fn socket_pair() -> Input {
        let (ours, peer) = UnixStream::pair().expect("a socket pair");
        Input { fd: ours.into(), keep: Box::new(peer) }
    }

    fn waiting(input: Option<Input>) -> (mpsc::Receiver<bool>, UnixStream, Option<Box<dyn std::any::Any + Send>>) {
        let (ours, server) = UnixStream::pair().expect("a socket pair");
        let (done, ended) = mpsc::channel();
        let (fd, keep) = match input {
            Some(Input { fd, keep }) => (Some(fd), Some(keep)),
            None => (None, None),
        };
        thread::spawn(move || {
            let mut ours = ours;
            let waited = wait_for_restart(&mut ours, fd.as_ref().map(AsFd::as_fd), false).expect("a wait");
            let _ = done.send(matches!(waited, Waited::InputEnded));
        });
        (ended, server, keep)
    }

    fn answer_and_restart(server: &mut UnixStream) {
        let report = serde_json::to_value(control::Report::default()).expect("json");
        let text = serde_json::to_string(&Response::Ok(report)).expect("json");
        protocol::send(server, &ServerMessage::Response(text)).expect("an answer");
        protocol::send(server, &ServerMessage::Restart("cornercase".into())).expect("a restart");
    }

    #[rstest]
    #[case::pipe(pipe())]
    #[case::socket_pair(socket_pair())]
    fn the_end_of_its_input_drops_the_connection(#[case] input: Input) {
        let (ended, mut server, keep) = waiting(Some(input));

        drop(keep);

        assert!(ended.recv_timeout(WAIT).expect("it ended"));
        assert!(matches!(protocol::recv::<protocol::ClientMessage>(&mut server), Ok(None)));
    }

    #[test]
    fn an_input_that_stays_lets_the_restart_happen() {
        let (ended, mut server, _keep) = waiting(Some(pipe()));

        server.flush().expect("flush");
        answer_and_restart(&mut server);

        assert!(!ended.recv_timeout(WAIT).expect("it ended"));
    }

    #[test]
    fn without_an_input_it_waits_as_before() {
        let (ended, mut server, _keep) = waiting(None);

        answer_and_restart(&mut server);

        assert!(!ended.recv_timeout(WAIT).expect("it ended"));
    }
}

mod the_json_line {
    use rstest::rstest;
    use serde_json::{Value, json};

    use super::*;

    fn updated() -> Ended {
        Ended::Updated { from: CURRENT, version: "9.0.0".into(), path: "/opt/bin/cornercase".into() }
    }

    #[rstest]
    #[case::updated(
        updated(),
        json!({"outcome": "updated", "from": CURRENT, "version": "9.0.0", "path": "/opt/bin/cornercase"})
    )]
    #[case::up_to_date(Ended::UpToDate { version: CURRENT }, json!({"outcome": "up-to-date", "version": CURRENT}))]
    #[case::manual(
        Ended::Manual { version: CURRENT, latest: "9.0.0".into(), command: update::BREW },
        json!({"outcome": "manual", "version": CURRENT, "latest": "9.0.0", "command": update::BREW})
    )]
    #[case::available(
        Ended::Available { version: CURRENT, latest: "9.0.0".into(), path: "/opt/bin/cornercase".into() },
        json!({"outcome": "available", "version": CURRENT, "latest": "9.0.0", "path": "/opt/bin/cornercase"})
    )]
    fn names_the_outcome_and_only_its_facts(#[case] ended: Ended, #[case] expected: Value) {
        assert_eq!(serde_json::to_value(&ended).expect("json"), expected);
    }

    #[test]
    fn keeps_the_issues_order_on_one_line() {
        let expected = format!(
            "{{\"outcome\":\"updated\",\"from\":\"{CURRENT}\",\"version\":\"9.0.0\",\"path\":\"/opt/bin/cornercase\"}}\n"
        );

        assert_eq!(json_line(&updated()).expect("a line"), expected);
    }
}

mod the_exit_status {
    use rstest::rstest;

    use super::*;

    fn manual() -> Ended {
        Ended::Manual { version: CURRENT, latest: "9.0.0".into(), command: update::BREW }
    }

    #[rstest]
    #[case::updated(Ended::Updated { from: CURRENT, version: "9.0.0".into(), path: String::new() }, false)]
    #[case::up_to_date(Ended::UpToDate { version: CURRENT }, false)]
    #[case::available(Ended::Available { version: CURRENT, latest: "9.0.0".into(), path: String::new() }, true)]
    #[case::manual_when_only_checking(manual(), true)]
    fn is_success(#[case] ended: Ended, #[case] check: bool) {
        assert!(status(&ended, check).expect("success"));
    }

    #[test]
    fn a_by_hand_update_has_a_status_of_its_own() {
        assert!(matches!(status(&manual(), false), Err(Error::UpdateByHand)));
    }
}

mod answering {
    use super::*;
    use crate::test_util::{FakeHttp, TempDir};

    const HOMEBREW: &str = "/opt/homebrew/Cellar/cornercase/0.1.0/bin/cornercase";

    fn latest(tag: &str) -> FakeHttp {
        FakeHttp::start(vec![("GET /releases/latest", 200, format!(r#"{{"tag_name": "{tag}", "assets": []}}"#))])
    }

    fn url(server: &FakeHttp) -> String {
        format!("{}/releases/latest", server.url())
    }

    #[test]
    fn a_check_names_where_it_would_install() {
        let server = latest("v99.0.0");
        let tmp = TempDir::new();
        let exe = tmp.path().join("cornercase");

        let ended = checking(&url(&server), &exe, Some("x86_64-unknown-linux-gnu")).expect("an answer");

        let path = exe.to_string_lossy().into_owned();
        assert_eq!(ended, Ended::Available { version: CURRENT, latest: "99.0.0".into(), path });
    }

    #[test]
    fn a_check_of_a_homebrew_install_gives_its_command() {
        let server = latest("v99.0.0");

        let ended = checking(&url(&server), Path::new(HOMEBREW), Some("x86_64-unknown-linux-gnu")).expect("an answer");

        assert_eq!(ended, Ended::Manual { version: CURRENT, latest: "99.0.0".into(), command: update::BREW });
    }

    #[test]
    fn a_check_on_an_unknown_platform_gives_the_installer() {
        let server = latest("v99.0.0");
        let tmp = TempDir::new();

        let ended = checking(&url(&server), &tmp.path().join("cornercase"), None).expect("an answer");

        assert_eq!(ended, Ended::Manual { version: CURRENT, latest: "99.0.0".into(), command: update::INSTALLER });
    }

    #[test]
    fn the_running_release_is_up_to_date() {
        let server = latest(&format!("v{CURRENT}"));
        let tmp = TempDir::new();
        let exe = tmp.path().join("cornercase");

        assert_eq!(checking(&url(&server), &exe, Some("x")).expect("an answer"), Ended::UpToDate { version: CURRENT });
        assert_eq!(
            installing(&url(&server), &exe, Some("x")).expect("an answer"),
            Ended::UpToDate { version: CURRENT }
        );
    }

    #[test]
    fn a_homebrew_install_is_left_alone() {
        let server = latest("v99.0.0");

        let ended =
            installing(&url(&server), Path::new(HOMEBREW), Some("x86_64-unknown-linux-gnu")).expect("an answer");

        assert_eq!(ended, Ended::Manual { version: CURRENT, latest: "99.0.0".into(), command: update::BREW });
    }

    #[cfg(unix)]
    #[test]
    fn an_install_says_where_it_went() {
        use crate::update::tests::replace::{NEW, installed, publish_for, sums_for};

        let tmp = TempDir::new();
        let target = update::target().expect("a known target");
        let (releases, _files) = publish_for(&tmp, target, |bytes| sums_for(target, bytes));
        let exe = installed(&tmp);

        let ended = installing(&url(&releases), &exe, Some(target)).expect("an answer");

        let path = exe.to_string_lossy().into_owned();
        assert_eq!(ended, Ended::Updated { from: CURRENT, version: "9.0.0".into(), path });
        assert_eq!(std::fs::read_to_string(&exe).expect("binary"), NEW);
    }
}
