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
