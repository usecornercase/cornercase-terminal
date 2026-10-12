use std::path::PathBuf;

use super::*;

#[test]
fn a_server_without_restart_when_idle_is_called_too_old_to_wait() {
    let refused = Error::Control(control::unknown_command("restart-when-idle"));

    assert_eq!(too_old_to_wait(refused).to_string(), TOO_OLD_TO_WAIT);
}

#[test]
fn an_input_thread_that_unwinds_tells_the_window_it_ended() {
    let (tx, rx) = mpsc::channel();
    crate::panics::contain(|| {
        let _ended = InputEnd(tx);
        panic!("an input thread bug");
    });
    assert!(matches!(rx.try_recv(), Ok(Incoming::InputEnded)));
}

mod probe {
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
}

mod restart {
    use super::*;

    fn restart() -> Received {
        Received::Message(ServerMessage::Restart(PathBuf::from("/tmp/not-cornercase")))
    }

    #[test]
    fn starts_this_binary_again_and_never_the_path_the_server_sends() {
        assert!(matches!(next(restart(), false), Ok(Next::End(Ending::Restart))));
    }

    #[test]
    fn reconnects_to_a_remote_server() {
        assert!(matches!(next(restart(), true), Ok(Next::Reconnect)));
    }
}

mod connection {
    use super::*;

    #[test]
    fn a_local_server_that_goes_away_ends_the_window() {
        assert!(matches!(next(Received::Ended, false), Ok(Next::End(Ending::Detached))));
    }

    #[test]
    fn a_remote_one_is_reconnected() {
        assert!(matches!(next(Received::Ended, true), Ok(Next::Reconnect)));
    }

    #[test]
    fn quit_ends_a_remote_window_too() {
        assert!(matches!(next(Received::Message(ServerMessage::Detached), true), Ok(Next::End(Ending::Detached))));
    }

    #[test]
    fn a_rejection_is_never_retried() {
        assert!(next(Received::Message(ServerMessage::Rejected("no".into())), true).is_err());
    }
}

mod reconnecting {
    use super::*;

    #[test]
    fn waits_longer_after_each_failure_up_to_a_limit() {
        let waits: Vec<u64> = (1..=7).map(|n| backoff(n).as_secs()).collect();

        assert_eq!(waits, [1, 2, 4, 8, 15, 15, 15]);
    }

    #[test]
    fn a_link_that_showed_the_screen_reconnects_at_once() {
        let mut retries = Retries::default();
        retries.linked();
        retries.showed();

        assert_eq!(retries.lost(), Duration::ZERO);
    }

    #[test]
    fn a_link_that_ends_before_its_first_frame_waits_longer_each_time() {
        let mut retries = Retries::default();
        let waits: Vec<Duration> = (0..3)
            .map(|_| {
                retries.linked();
                retries.lost()
            })
            .collect();

        assert_eq!(waits, [SECOND, SECOND * 2, SECOND * 4]);
    }

    #[test]
    fn failed_attempts_and_early_ends_add_up() {
        let mut retries = Retries::default();
        retries.linked();
        retries.showed();
        retries.lost();
        retries.failed();
        retries.linked();

        assert_eq!(retries.lost(), SECOND * 2);
    }

    #[test]
    fn counts_down_in_whole_seconds() {
        assert_eq!(seconds(Duration::from_millis(3400)), 4);
        assert_eq!(seconds(Duration::ZERO), 0);
        assert_eq!(until_next_second(Duration::from_millis(3400)), Duration::from_millis(400));
        assert_eq!(until_next_second(Duration::from_secs(3)), SECOND);
    }
}
