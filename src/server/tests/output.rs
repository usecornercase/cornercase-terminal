use std::cell::Cell;

use super::a_bug::Attached;
use super::*;

mod event_batches {
    use super::*;

    #[test]
    fn a_busy_batch_yields_without_losing_the_next_event() {
        let mut attached = Attached::new();
        let (tx, rx) = mpsc::channel();
        tx.send(ServerEvent::Gone(2)).expect("queue another event");
        let handled = Cell::new(0);
        let run = |_: &mut Server, _: Step| {
            handled.set(handled.get() + 1);
            let started = Instant::now();
            while started.elapsed() < FRAME {
                std::hint::spin_loop();
            }
            ControlFlow::Continue(())
        };

        let flow = attached.server.run_events(Some(ServerEvent::Gone(1)), &rx, &run);

        assert_eq!(flow, ControlFlow::Continue(()));
        assert_eq!(handled.get(), 1);
        assert!(matches!(rx.try_recv(), Ok(ServerEvent::Gone(2))));
    }
}
