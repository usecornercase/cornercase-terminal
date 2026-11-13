use std::sync::mpsc;

use super::*;

const FORWARD: Held = Held { port: 5173, local: 5174, direction: None };
const REVERSE: Held = Held { port: 9223, local: 9222, direction: Some(Direction::Reverse) };

fn heard(events: Vec<Heard>) -> Receiver<Heard> {
    let (tell, heard) = mpsc::channel();
    for event in events {
        tell.send(event).expect("sent");
    }
    heard
}

mod waiting_for_it_to_come_up {
    use super::*;

    #[test]
    fn goes_on_once_ssh_listens() {
        assert!(wait_up(&heard(vec![Heard::Up]), FORWARD, |_| {}).expect("up"));
    }

    #[test]
    fn fails_with_the_reason_when_ssh_ends_first() {
        let error = wait_up(&heard(vec![Heard::Ended("refused".into())]), FORWARD, |_| {}).expect_err("ended");

        assert_eq!(error.to_string(), "could not tunnel port 5173: refused");
    }

    #[test]
    fn reports_ssh_s_reason_without_the_prefix_when_ssh_ends_first() {
        let mut reported = Vec::new();

        let _ =
            wait_up(&heard(vec![Heard::Ended("refused".into())]), FORWARD, |reason| reported.push(reason.to_string()));

        assert_eq!(reported, ["refused"]);
    }

    #[test]
    fn reports_nothing_when_ssh_comes_up_or_a_signal_comes() {
        let mut reported = Vec::new();

        let _ = wait_up(&heard(vec![Heard::Up]), FORWARD, |reason| reported.push(reason.to_string()));
        let _ = wait_up(&heard(vec![Heard::Signal]), FORWARD, |reason| reported.push(reason.to_string()));

        assert_eq!(reported, Vec::<String>::new());
    }

    #[test]
    fn stops_quietly_when_a_signal_comes_first() {
        assert!(!wait_up(&heard(vec![Heard::Signal]), FORWARD, |_| {}).expect("signalled"));
    }
}

mod waiting_for_the_registration {
    use super::*;

    #[test]
    fn goes_on_once_the_server_answered() {
        assert!(wait_registered(&heard(vec![Heard::Registered]), FORWARD, |_| {}).expect("registered"));
    }

    #[test]
    fn stops_quietly_on_a_signal_or_an_ended_stream() {
        assert!(!wait_registered(&heard(vec![Heard::Signal]), FORWARD, |_| {}).expect("signalled"));
        assert!(!wait_registered(&heard(vec![Heard::Stream(Ok(()))]), FORWARD, |_| {}).expect("ended"));
    }

    #[test]
    fn fails_when_the_server_refuses_or_ssh_ends() {
        assert!(wait_registered(&heard(vec![Heard::Stream(Err(Error::ServerLost))]), FORWARD, |_| {}).is_err());
        assert!(wait_registered(&heard(vec![Heard::Ended("dropped".into())]), FORWARD, |_| {}).is_err());
    }
}

mod reporting_before_the_registration {
    use super::*;

    #[test]
    fn a_failure_before_the_registration_is_reported_with_ssh_s_reason() {
        let mut reported = Vec::new();

        let _ = wait_registered(&heard(vec![Heard::Ended("dropped".into())]), FORWARD, |reason| {
            reported.push(reason.to_string());
        });

        assert_eq!(reported, ["dropped"]);
    }

    #[test]
    fn nothing_is_reported_once_it_is_registered() {
        let mut reported = Vec::new();

        let _ = wait_registered(&heard(vec![Heard::Registered]), FORWARD, |reason| reported.push(reason.to_string()));
        let _ = held(&heard(vec![Heard::Ended("dropped".into())]), FORWARD);

        assert_eq!(reported, Vec::<String>::new());
    }
}

mod holding_it {
    use super::*;

    #[test]
    fn ends_with_success_when_the_stream_ends_or_a_signal_comes() {
        assert!(held(&heard(vec![Heard::Stream(Ok(()))]), FORWARD).expect("closed"));
        assert!(held(&heard(vec![Heard::Signal]), FORWARD).expect("killed"));
    }

    #[test]
    fn fails_when_ssh_ends_after_it_was_up() {
        let error = held(&heard(vec![Heard::Ended("dropped".into())]), FORWARD).expect_err("ended");

        assert_eq!(error.to_string(), "the tunnel of port 5173 ended: dropped");
    }

    #[test]
    fn fails_when_the_server_connection_is_lost() {
        let error = held(&heard(vec![Heard::Stream(Err(Error::ServerLost))]), FORWARD).expect_err("lost");

        assert!(matches!(error, Error::ServerLost));
    }
}

mod announcing_it {
    use super::*;

    #[test]
    fn prints_one_compact_json_line() {
        let mut out = Vec::new();

        assert!(!announce(&mut out, FORWARD).expect("written"));

        let expected = "{\"port\":5173,\"local\":5174,\"url\":\"http://localhost:5174\"}\n";
        assert_eq!(String::from_utf8(out).expect("text"), expected);
    }

    #[test]
    fn prints_a_reverse_line_with_its_direction_and_the_machine_s_address() {
        let mut out = Vec::new();

        assert!(!announce(&mut out, REVERSE).expect("written"));

        let expected = "{\"port\":9223,\"local\":9222,\"url\":\"http://127.0.0.1:9223\",\"direction\":\"reverse\"}\n";
        assert_eq!(String::from_utf8(out).expect("text"), expected);
    }

    struct Closed;

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn stops_when_the_reader_is_gone() {
        assert!(announce(&mut Closed, FORWARD).expect("quiet"));
    }
}

mod holding_a_reverse_tunnel {
    use super::*;

    #[test]
    fn says_which_port_of_this_computer_could_not_be_held() {
        let asked = Held { port: 0, ..REVERSE };

        let error = wait_up(&heard(vec![Heard::Ended("refused".into())]), asked, |_| {}).expect_err("ended");

        assert_eq!(error.to_string(), "could not hold a reverse tunnel to port 9222: refused");
    }

    #[test]
    fn names_the_machine_s_port_when_it_ends() {
        let error = held(&heard(vec![Heard::Ended("dropped".into())]), REVERSE).expect_err("ended");

        assert_eq!(error.to_string(), "the reverse tunnel of port 9223 ended: dropped");
    }
}
