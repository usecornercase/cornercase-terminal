use std::cell::RefCell;

use super::*;

struct Fake {
    askable: bool,
    ends_on: Option<Signal>,
    refused: Option<&'static str>,
    sent: RefCell<Vec<Signal>>,
}

impl Fake {
    fn new(askable: bool, ends_on: Option<Signal>) -> Self {
        Self { askable, ends_on, refused: None, sent: RefCell::default() }
    }

    fn sent(&self) -> Vec<Signal> {
        self.sent.borrow().clone()
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
        self.sent.borrow_mut().push(signal);
        Ok(())
    }

    fn holds(&self) -> bool {
        self.ends_on.is_none_or(|end| !self.sent.borrow().contains(&end))
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
fn a_target_that_survives_the_hard_step_is_still_running() {
    let (target, outcome) = one(Fake::new(false, None));
    assert_eq!((outcome, target.sent()), (Ok(Outcome::StillRunning), vec![Signal::KILL]));
}

#[test]
fn a_refused_target_gets_nothing() {
    let (target, outcome) = one(Fake { refused: Some("gone"), ..Fake::new(true, Some(Signal::TERM)) });
    assert_eq!((outcome, target.sent()), (Err("gone".into()), Vec::new()));
}
