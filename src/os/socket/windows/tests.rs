use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use windows_sys::Win32::Foundation::{GetHandleInformation, HANDLE_FLAG_INHERIT};

use super::*;
use crate::os::fs::uid;
use crate::protocol::{self, ClientMessage, ServerMessage};
use crate::test_util::TempDir;

const WAIT: Duration = Duration::from_secs(10);
const SHORT: Duration = Duration::from_millis(50);

fn inherited(socket: &Socket) -> bool {
    let mut flags = 0;
    let read = unsafe { GetHandleInformation(socket.raw as HANDLE, &raw mut flags) };
    assert_ne!(read, 0, "the handle's flags");
    flags & HANDLE_FLAG_INHERIT != 0
}

fn read_all(mut stream: UnixStream) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut got = Vec::new();
        let _ = tx.send(stream.read_to_end(&mut got).map(|_| got));
    });
    rx
}

fn reading_after_a_byte(mut stream: UnixStream) -> (mpsc::Receiver<()>, mpsc::Receiver<io::Result<Vec<u8>>>) {
    let (began, reading) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Err(error) = stream.read_exact(&mut [0; 1]) {
            let _ = tx.send(Err(error));
            return;
        }
        let _ = began.send(());
        let mut rest = Vec::new();
        let _ = tx.send(stream.read_to_end(&mut rest).map(|_| rest));
    });
    (reading, rx)
}

fn still_reading(read: &mpsc::Receiver<io::Result<Vec<u8>>>, within: Duration) -> bool {
    matches!(read.recv_timeout(within), Err(RecvTimeoutError::Timeout))
}

mod a_listener {
    use super::*;

    #[test]
    fn exchanges_frames_with_a_stream() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let server = thread::spawn(move || {
            let mut stream = listener.incoming().next().expect("a connection").expect("accepted");
            let Ok(Some(ClientMessage::Request(text))) = protocol::recv(&mut stream) else { panic!("a request") };
            protocol::send(&mut stream, &ServerMessage::Response(format!("{text} back"))).expect("an answer");
        });

        let mut client = UnixStream::connect(&path).expect("connect");
        protocol::send(&mut client, &ClientMessage::Request("status".into())).expect("a request");
        let answer = protocol::recv::<ServerMessage>(&mut client).expect("an answer");

        server.join().expect("the server");
        assert!(matches!(answer, Some(ServerMessage::Response(text)) if text == "status back"));
        assert!(matches!(protocol::recv::<ServerMessage>(&mut client), Ok(None)));
    }

    #[test]
    fn binds_in_a_folder_whose_name_is_not_ascii() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("Usuário ñ");
        std::fs::create_dir(&dir).expect("folder");
        let path = dir.join("server.sock");
        let _listener = UnixListener::bind(&path).expect("bind");

        assert!(UnixStream::connect(&path).is_ok());
        assert!(path.exists());
    }

    #[test]
    fn leaves_a_file_that_refuses_connections_once_closed() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.sock");
        drop(UnixListener::bind(&path).expect("bind"));

        let refused = UnixStream::connect(&path).expect_err("nobody listens");

        assert_eq!(refused.kind(), io::ErrorKind::ConnectionRefused);
        assert!(path.exists());
        assert_eq!(UnixListener::bind(&path).expect_err("taken").kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn refuses_a_path_longer_than_the_address_holds() {
        let tmp = TempDir::new();
        let path = tmp.path().join("x".repeat(108));

        assert_eq!(UnixListener::bind(&path).expect_err("too long").kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn its_sockets_are_not_inherited_by_children() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let client = UnixStream::connect(&path).expect("connect");
        let accepted = listener.incoming().next().expect("a connection").expect("accepted");

        assert!(!inherited(&listener.0));
        assert!(!inherited(&client.0));
        assert!(!inherited(&accepted.0));
    }
}

mod a_pair {
    use super::*;

    #[test]
    fn talks_both_ways() {
        let (mut ours, mut theirs) = UnixStream::pair().expect("a pair");

        ours.write_all(b"ping").expect("write");
        let mut got = [0; 4];
        theirs.read_exact(&mut got).expect("read");
        theirs.write_all(b"pong").expect("write back");
        let mut back = [0; 4];
        ours.read_exact(&mut back).expect("read back");

        assert_eq!((&got, &back), (b"ping", b"pong"));
    }

    #[test]
    fn ends_for_one_side_when_the_other_is_dropped() {
        let (ours, mut theirs) = UnixStream::pair().expect("a pair");

        theirs.write_all(b"last").expect("write");
        drop(theirs);

        assert_eq!(read_all(ours).recv_timeout(WAIT).expect("read").expect("the end"), b"last");
    }

    #[test]
    fn belongs_to_this_user() {
        let (ours, theirs) = UnixStream::pair().expect("a pair");

        assert_eq!(peer_uid(&ours).expect("the peer"), uid());
        assert_eq!(peer_uid(&theirs).expect("the peer"), uid());
    }

    #[test]
    fn shares_one_connection_between_its_clones() {
        let (ours, theirs) = UnixStream::pair().expect("a pair");
        let mut clone = ours.try_clone().expect("a clone");

        clone.write_all(b"from the clone").expect("write");
        drop(clone);
        ours.shutdown(Shutdown::Write).expect("shutdown");

        assert_eq!(read_all(theirs).recv_timeout(WAIT).expect("read").expect("the end"), b"from the clone");
    }
}

mod a_read_timeout {
    use super::*;

    #[test]
    fn ends_a_read_that_waits_too_long() {
        let (mut ours, _theirs) = UnixStream::pair().expect("a pair");
        ours.set_read_timeout(Some(SHORT)).expect("a timeout");
        let started = Instant::now();

        let error = ours.read(&mut [0; 4]).expect_err("timed out");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < WAIT);
    }

    #[test]
    fn of_none_waits_again() {
        let (ours, mut theirs) = UnixStream::pair().expect("a pair");
        ours.set_read_timeout(Some(SHORT)).expect("a timeout");
        ours.set_read_timeout(None).expect("no timeout");
        let (reading, read) = reading_after_a_byte(ours);
        theirs.write_all(b"1").expect("the first byte");
        reading.recv_timeout(WAIT).expect("the reader reads");

        assert!(still_reading(&read, SHORT * 4), "the read ended although no timeout is set");
        theirs.write_all(b"late").expect("write");
        drop(theirs);

        assert_eq!(read.recv_timeout(WAIT).expect("read").expect("the rest"), b"late");
    }

    #[test]
    fn of_zero_is_refused_as_std_does() {
        let (ours, _theirs) = UnixStream::pair().expect("a pair");

        let error = ours.set_read_timeout(Some(Duration::ZERO)).expect_err("refused");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn of_a_write_is_set_too() {
        let (ours, _theirs) = UnixStream::pair().expect("a pair");

        ours.set_write_timeout(Some(Duration::from_secs(1))).expect("a write timeout");
    }
}

mod shutting_down {
    use super::*;

    #[test]
    fn the_write_side_ends_what_the_peer_reads() {
        let (mut ours, theirs) = UnixStream::pair().expect("a pair");
        let read = read_all(theirs);

        ours.write_all(b"done").expect("write");
        ours.shutdown(Shutdown::Write).expect("shutdown");

        assert_eq!(read.recv_timeout(WAIT).expect("read").expect("the end"), b"done");
    }

    #[test]
    fn both_sides_wakes_a_read_blocked_on_a_clone() {
        let (ours, mut theirs) = UnixStream::pair().expect("a pair");
        let (reading, read) = reading_after_a_byte(ours.try_clone().expect("a clone"));
        theirs.write_all(b"1").expect("the first byte");
        reading.recv_timeout(WAIT).expect("the reader reads");
        assert!(still_reading(&read, SHORT), "the read waits for more");

        ours.shutdown(Shutdown::Both).expect("shutdown");

        assert_eq!(read.recv_timeout(WAIT).expect("woken").expect("an end, not an error"), b"");
    }

    #[test]
    fn the_read_side_reads_as_the_end() {
        let (mut ours, mut theirs) = UnixStream::pair().expect("a pair");

        ours.shutdown(Shutdown::Read).expect("shutdown");
        let _ = theirs.write_all(b"ignored");

        assert_eq!(ours.read(&mut [0; 8]).expect("an end"), 0);
    }

    #[test]
    fn tells_the_peer_the_connection_is_over() {
        let (ours, theirs) = UnixStream::pair().expect("a pair");
        let read = read_all(theirs);

        ours.shutdown(Shutdown::Both).expect("shutdown");

        assert_eq!(read.recv_timeout(WAIT).expect("read").expect("the end"), b"");
    }
}
