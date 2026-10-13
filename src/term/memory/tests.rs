use super::*;
use crate::test_util::{Family, exited_pid, this_pid, wait_until};

fn measured(pid: i32, now: Instant) -> Pane {
    let mut pane = Pane::default();
    wait_until("the memory is measured", || {
        pane.update(pid, now);
        pane.bytes().is_some()
    });
    pane
}

#[test]
fn counts_the_process_and_everything_it_started() {
    let family = Family::new();
    let parent =
        [family.pid(), family.child().expect("a child")].map(|pid| process::footprint(pid).expect("footprint"));

    let pane = measured(family.pid(), Instant::now());

    assert!(pane.bytes() > Some(parent.iter().sum()), "{:?} <= {parent:?}", pane.bytes());
}

#[test]
fn measures_again_once_two_seconds_have_passed() {
    let pid = this_pid();
    let now = Instant::now();
    let mut pane = measured(pid, now);

    pane.update(pid, now + MEASURE_EVERY.saturating_sub(Duration::from_millis(1)));
    let early = pane.measuring.is_some();
    pane.update(pid, now + MEASURE_EVERY);

    assert_eq!((early, pane.measuring.is_some()), (false, true));
}

#[test]
fn a_process_that_exited_has_no_memory() {
    let (pid, mut pane) = (exited_pid(), Pane::default());

    wait_until("the measure ends", || {
        pane.update(pid, Instant::now());
        pane.measuring.is_none()
    });

    assert_eq!(pane.bytes(), None);
}
