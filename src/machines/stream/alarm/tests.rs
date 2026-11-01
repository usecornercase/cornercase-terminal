use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::test_util;

#[derive(Default)]
struct Cut(AtomicBool);

impl Cut {
    fn was_cut(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Ends for Cut {
    fn cut(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    fn why(&self) -> Option<String> {
        None
    }
}

fn ends(cut: &Arc<Cut>) -> Arc<dyn Ends> {
    Arc::clone(cut) as Arc<dyn Ends>
}

#[test]
fn one_timer_rings_every_alarm_that_is_due() {
    let alarms = Alarms::new(Arc::new(Timer::new()), Duration::ZERO);
    let (first, second) = (Arc::new(Cut::default()), Arc::new(Cut::default()));

    let set = [alarms.set(ends(&first)), alarms.set(ends(&second))];

    test_util::wait_until("both alarms ring", || first.was_cut() && second.was_cut());
    assert!(set.iter().all(Alarm::rang));
}

#[test]
fn an_alarm_turned_off_never_rings() {
    let timer = Timer::new();
    let (off, on) = (Arc::new(Cut::default()), Arc::new(Cut::default()));
    let quiet = Arc::new(Armed::default());
    quiet.off();

    timer.arm(Arc::clone(&quiet), ends(&off), Duration::ZERO);
    timer.arm(Arc::new(Armed::default()), ends(&on), Duration::ZERO);

    test_util::wait_until("the alarm still on rings", || on.was_cut());
    assert!(!off.was_cut() && !quiet.rang());
}
