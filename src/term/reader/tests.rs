use std::assert_matches;
use std::io::Cursor;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use super::*;

const TIMEOUT: Duration = Duration::from_secs(5);

fn output(rx: &Receiver<Event>) -> Output {
    let Event::Output(7, 3, bytes) = rx.recv_timeout(TIMEOUT).expect("output arrives") else {
        panic!("expected output from this pane and generation");
    };
    bytes
}

mod backpressure {
    use super::*;

    struct Tracked {
        input: Cursor<Vec<u8>>,
        closed: Sender<()>,
    }

    impl Read for Tracked {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(bytes)
        }
    }

    impl Drop for Tracked {
        fn drop(&mut self) {
            let _ = self.closed.send(());
        }
    }

    #[test]
    fn taking_events_out_of_the_queue_does_not_release_the_reader() {
        let (tx, rx) = mpsc::channel();
        spawn(7, 3, Box::new(Cursor::new(vec![b'x'; READ_BUFFER * (OUTPUT_CHUNKS + 1)])), tx);
        let mut held: Vec<_> = (0..OUTPUT_CHUNKS).map(|_| output(&rx)).collect();

        assert_matches!(rx.recv_timeout(Duration::from_millis(100)), Err(RecvTimeoutError::Timeout));
        drop(held.pop());
        let next = output(&rx);

        assert_eq!(&*next, vec![b'x'; READ_BUFFER]);
        drop(next);
        assert_matches!(rx.recv_timeout(TIMEOUT), Ok(Event::Exited(7, 3)));
    }

    #[test]
    fn every_byte_arrives_in_order_before_the_exit() {
        let input: Vec<_> = (0u8..=255).cycle().take(READ_BUFFER * (OUTPUT_CHUNKS + 2) + 17).collect();
        let (tx, rx) = mpsc::channel();
        spawn(7, 3, Box::new(Cursor::new(input.clone())), tx);
        let mut received = Vec::new();

        loop {
            match rx.recv_timeout(TIMEOUT).expect("output or exit arrives") {
                Event::Output(7, 3, bytes) => received.extend_from_slice(&bytes),
                Event::Exited(7, 3) => break,
                other => panic!("unexpected event: {other:?}"),
            }
        }

        assert_eq!(received, input);
    }

    #[test]
    fn dropping_a_full_queue_releases_the_reader() {
        let (tx, rx) = mpsc::channel();
        let (closed, finished) = mpsc::channel();
        let input = Cursor::new(vec![b'x'; READ_BUFFER * (OUTPUT_CHUNKS + 1)]);
        spawn(7, 3, Box::new(Tracked { input, closed }), tx);
        let held: Vec<_> = (0..OUTPUT_CHUNKS).map(|_| output(&rx)).collect();

        drop(rx);
        drop(held);

        finished.recv_timeout(TIMEOUT).expect("the blocked reader closes");
    }

    #[test]
    fn a_full_pane_does_not_hold_back_another_pane() {
        let (full_tx, full_rx) = mpsc::channel();
        spawn(7, 3, Box::new(Cursor::new(vec![b'x'; READ_BUFFER * (OUTPUT_CHUNKS + 1)])), full_tx);
        let held: Vec<_> = (0..OUTPUT_CHUNKS).map(|_| output(&full_rx)).collect();
        let (tx, rx) = mpsc::channel();

        spawn(7, 3, Box::new(Cursor::new(b"another pane")), tx);

        assert_eq!(&*output(&rx), b"another pane");
        assert_matches!(rx.recv_timeout(TIMEOUT), Ok(Event::Exited(7, 3)));
        drop(held);
    }
}
