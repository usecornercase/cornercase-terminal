use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::test_util::wait_until;

struct Counting {
    produced: Arc<AtomicUsize>,
    total: usize,
}

impl Read for Counting {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.produced.load(Ordering::SeqCst) >= self.total {
            return Ok(0);
        }
        self.produced.fetch_add(1, Ordering::SeqCst);
        buffer[..2].copy_from_slice(b"x\n");
        Ok(2)
    }
}

#[test]
fn a_reader_far_ahead_of_its_consumer_waits_instead_of_queueing_everything() {
    let produced = Arc::new(AtomicUsize::new(0));
    let total = QUEUED * 4;

    let lines = read(Counting { produced: produced.clone(), total });

    wait_until("the queue is full", || produced.load(Ordering::SeqCst) > QUEUED);
    assert_eq!(produced.load(Ordering::SeqCst), QUEUED + 1);
    assert_eq!(lines.iter().count(), total);
}
