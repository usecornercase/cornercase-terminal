use std::cell::RefCell;

use super::*;

struct Fake {
    askable: bool,
    ends_on: Option<Signal>,
    refused: Option<&'static str>,
    fails: Option<Signal>,
    sent: RefCell<Vec<(Signal, Instant)>>,
}

impl Fake {
    fn new(askable: bool, ends_on: Option<Signal>) -> Self {
        Self { askable, ends_on, refused: None, fails: None, sent: RefCell::default() }
    }

    fn sent(&self) -> Vec<Signal> {
        self.sent.borrow().iter().map(|(signal, _)| *signal).collect()
    }

    fn sent_at(&self, wanted: Signal) -> Option<Instant> {
        self.sent.borrow().iter().find(|(signal, _)| *signal == wanted).map(|(_, at)| *at)
    }
}

impl Stoppable for Fake {
    fn check(&self) -> Result<(), String> {
        self.refused.map_or(Ok(()), |reason| Err(reason.into()))
    }

    fn askable(&self) -> bool {
        self.askable
    }

    fn signal(&self, signal: Signal) -> Result<(), String> {
        self.sent.borrow_mut().push((signal, Instant::now()));
        if self.fails == Some(signal) { Err("the request could not be sent".into()) } else { Ok(()) }
    }

    fn holds(&self) -> bool {
        self.ends_on.is_none_or(|end| !self.sent().contains(&end))
    }
}

fn one(target: Fake) -> (Fake, Result<Outcome, String>) {
    let mut stopped = escalate(vec![target]);
    assert_eq!(stopped.len(), 1);
    stopped.remove(0)
}

#[test]
fn a_target_that_ends_when_asked_is_stopped_without_the_hard_step() {
    let (target, outcome) = one(Fake::new(true, Some(Signal::TERM)));
    assert_eq!((outcome, target.sent()), (Ok(Outcome::Stopped), vec![Signal::TERM]));
}

#[test]
fn a_target_that_cannot_be_asked_gets_the_hard_step_at_once() {
    let started = Instant::now();
    let (target, outcome) = one(Fake::new(false, Some(Signal::KILL)));
    assert_eq!((outcome, target.sent()), (Ok(Outcome::Killed), vec![Signal::KILL]));
    assert!(started.elapsed() < GRACE, "{:?}", started.elapsed());
}

#[test]
fn a_target_that_cannot_be_asked_does_not_wait_for_the_others_grace() {
    let waiting = Fake::new(true, Some(Signal::KILL));
    let unasked = Fake::new(false, Some(Signal::KILL));
    let started = Instant::now();

    let stopped = escalate(vec![waiting, unasked]);

    let killed_at = |askable: bool| {
        let (target, _) = stopped.iter().find(|(target, _)| target.askable == askable).expect("a target");
        target.sent_at(Signal::KILL).expect("killed").duration_since(started)
    };
    assert!(stopped.iter().all(|(_, outcome)| *outcome == Ok(Outcome::Killed)), "{:?}", stopped.len());
    assert!(killed_at(false) < GRACE, "{:?}", killed_at(false));
    assert!(killed_at(true) >= GRACE, "{:?}", killed_at(true));
}

#[test]
fn a_target_that_survives_the_hard_step_is_still_running() {
    let (target, outcome) = one(Fake::new(false, None));
    assert_eq!((outcome, target.sent()), (Ok(Outcome::StillRunning), vec![Signal::KILL]));
}

#[test]
fn a_refused_target_gets_nothing() {
    let (target, outcome) = one(Fake { refused: Some("gone"), ..Fake::new(true, Some(Signal::TERM)) });
    assert_eq!((outcome, target.sent()), (Err("gone".into()), Vec::new()));
}

#[test]
fn a_request_that_cannot_be_sent_ends_the_stop_on_unix_and_goes_to_the_hard_step_on_windows() {
    let started = Instant::now();

    let (target, outcome) = one(Fake { fails: Some(Signal::TERM), ..Fake::new(true, Some(Signal::KILL)) });

    if cfg!(windows) {
        assert_eq!((outcome, target.sent()), (Ok(Outcome::Killed), vec![Signal::TERM, Signal::KILL]));
        assert!(started.elapsed() < GRACE, "{:?}", started.elapsed());
    } else {
        let refused = Err("the request could not be sent".to_string());
        assert_eq!((outcome, target.sent()), (refused, vec![Signal::TERM]));
    }
}
