use std::fmt::Display;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Order, Update, Via};
use crate::log;
use crate::panics;

const NAP: Duration = Duration::from_millis(100);
pub(crate) const MOST_WAIT: u64 = 60;
const TRIES: usize = 3;

#[derive(Debug)]
pub(crate) enum Event {
    Updates(u64, Vec<Update>),
    Sent { link: u64, tag: u64, message: i64 },
    Problem(u64, Option<String>),
    Rejected(u64),
}

pub(crate) enum Next<F> {
    Wait(u64),
    Again,
    Done(Result<Option<i64>, F>),
}

pub(crate) trait Transport: Send + 'static {
    type Failure: Display + From<String>;
    type Attempt: Default;

    fn perform(&self, order: &Order, attempt: &Self::Attempt) -> Result<Option<i64>, Self::Failure>;

    fn next(failure: Self::Failure, attempt: &mut Self::Attempt) -> Next<Self::Failure>;

    fn method(order: &Order) -> &'static str;

    fn more(_attempt: &Self::Attempt) -> Vec<i64> {
        Vec::new()
    }
}

pub(crate) struct Link {
    pub(crate) id: u64,
    orders: Sender<Order>,
    stop: Arc<AtomicBool>,
}

impl Link {
    pub(crate) fn start<E, T>(
        id: u64,
        via: Via,
        tx: &Sender<E>,
        transport: T,
        listen: impl FnOnce(&AtomicBool) + Send + 'static,
    ) -> Self
    where
        E: From<(Via, Event)> + Send + 'static,
        T: Transport,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let (orders, received) = mpsc::channel();
        let send_tx = tx.clone();
        let (listen_stop, send_stop) = (Arc::clone(&stop), Arc::clone(&stop));
        thread::spawn(move || listen(&listen_stop));
        thread::spawn(move || deliver(id, via, &transport, &received, &send_tx, &send_stop));
        Self { id, orders, stop }
    }

    pub(crate) fn send(&self, order: Order) {
        let _ = self.orders.send(order);
    }

    #[cfg(test)]
    pub(crate) fn capture(id: u64) -> (Self, Receiver<Order>) {
        let (orders, received) = mpsc::channel();
        (Self { id, orders, stop: Arc::new(AtomicBool::new(false)) }, received)
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn nap(stop: &AtomicBool, wait: Duration) {
    let until = Instant::now() + wait;
    while !stop.load(Ordering::Relaxed) && Instant::now() < until {
        thread::sleep(NAP.min(until.saturating_duration_since(Instant::now())));
    }
}

pub(crate) fn backoff(failed: u32) -> Duration {
    Duration::from_secs(1u64.checked_shl(failed.saturating_sub(1)).unwrap_or(MOST_WAIT).min(MOST_WAIT))
}

fn perform<T: Transport>(transport: &T, order: &Order, stop: &AtomicBool) -> Result<Vec<i64>, T::Failure> {
    let mut attempt = T::Attempt::default();
    let sent = |done: Result<Option<i64>, T::Failure>, attempt: &T::Attempt| {
        done.map(|first| first.map_or_else(Vec::new, |first| [first].into_iter().chain(T::more(attempt)).collect()))
    };
    for _ in 0..TRIES {
        match transport.perform(order, &attempt) {
            Err(failure) => match T::next(failure, &mut attempt) {
                Next::Wait(seconds) => nap(stop, Duration::from_secs(seconds.min(MOST_WAIT))),
                Next::Again => {}
                Next::Done(done) => return sent(done, &attempt),
            },
            done => return sent(done, &attempt),
        }
    }
    let done = transport.perform(order, &attempt);
    sent(done, &attempt)
}

fn deliver<E: From<(Via, Event)>, T: Transport>(
    id: u64,
    via: Via,
    transport: &T,
    orders: &Receiver<Order>,
    tx: &Sender<E>,
    stop: &AtomicBool,
) {
    for order in orders {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let done = panics::job(|| Ok(perform(transport, &order, stop)))
            .unwrap_or_else(|e| Err(T::Failure::from(e.to_string())));
        match done {
            Ok(messages) => {
                let Some(tag) = order.tag() else { continue };
                for message in messages {
                    if tx.send((via, Event::Sent { link: id, tag, message }).into()).is_err() {
                        return;
                    }
                }
            }
            Err(failure) => log::warning!(via.target(), "not delivered", method = T::method(&order), error = failure),
        }
    }
}
