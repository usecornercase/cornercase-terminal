use std::os::unix::net::UnixStream;

use super::*;

#[test]
fn a_reply_queued_when_the_deadline_passes_is_read_and_dropped() {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    theirs.write_all(b"\x1b_Gi=31;OK\x1b\\").expect("write the late reply");
    let probe = listen(&ours, Duration::ZERO);
    assert!(!probe.is_done());
    assert!(!readable(&ours, Duration::ZERO), "the late reply would reach a pane");
}

#[test]
fn a_probe_that_ends_on_its_marker_waits_for_nothing_more() {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    theirs.write_all(b"\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c").expect("write the replies");
    let started = Instant::now();

    let probe = listen(&ours, THEME_QUERY_TIMEOUT);

    assert!(probe.is_done() && started.elapsed() < DRAIN_QUIET);
}
