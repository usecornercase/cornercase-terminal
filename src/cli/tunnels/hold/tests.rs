use std::sync::mpsc;

use super::*;

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
        assert!(wait_up(&heard(vec![Heard::Up]), 5173).expect("up"));
    }

    #[test]
    fn fails_with_the_reason_when_ssh_ends_first() {
        let error = wait_up(&heard(vec![Heard::Ended("refused".into())]), 5173).expect_err("ended");

        assert_eq!(error.to_string(), "could not tunnel port 5173: refused");
    }

    #[test]
    fn stops_quietly_when_a_signal_comes_first() {
        assert!(!wait_up(&heard(vec![Heard::Signal]), 5173).expect("signalled"));
    }
}

mod holding_it {
    use super::*;

    #[test]
    fn ends_with_success_when_the_stream_ends_or_a_signal_comes() {
        assert!(held(&heard(vec![Heard::Stream(Ok(()))]), 5173).expect("closed"));
        assert!(held(&heard(vec![Heard::Signal]), 5173).expect("killed"));
    }

    #[test]
    fn fails_when_ssh_ends_after_it_was_up() {
        let error = held(&heard(vec![Heard::Ended("dropped".into())]), 5173).expect_err("ended");

        assert_eq!(error.to_string(), "the tunnel of port 5173 ended: dropped");
    }

    #[test]
    fn fails_when_the_server_connection_is_lost() {
        let error = held(&heard(vec![Heard::Stream(Err(Error::ServerLost))]), 5173).expect_err("lost");

        assert!(matches!(error, Error::ServerLost));
    }
}

mod announcing_it {
    use super::*;

    #[test]
    fn prints_one_compact_json_line() {
        let mut out = Vec::new();

        assert!(!announce(&mut out, 5173, 5174).expect("written"));

        let expected = "{\"port\":5173,\"local\":5174,\"url\":\"http://localhost:5174\"}\n";
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
        assert!(announce(&mut Closed, 5173, 5174).expect("quiet"));
    }
}
