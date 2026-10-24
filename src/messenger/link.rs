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
    Posted { link: u64, tag: u64, thread: i64, message: i64 },
    Failed { link: u64, tag: u64 },
    Archived { link: u64, thread: i64 },
    Gone { link: u64, thread: i64, order: Order },
    Problem(u64, Option<String>),
    DirectOnly(u64),
    Rejected(u64),
}

impl Event {
    pub(crate) fn link(&self) -> u64 {
        match self {
            Self::Updates(link, _)
            | Self::Problem(link, _)
            | Self::Rejected(link)
            | Self::DirectOnly(link)
            | Self::Sent { link, .. }
            | Self::Posted { link, .. }
            | Self::Failed { link, .. }
            | Self::Archived { link, .. }
            | Self::Gone { link, .. } => *link,
        }
    }
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

    fn posted(_attempt: &Self::Attempt) -> Option<i64> {
        None
    }

    fn gone(_order: &Order, _failure: &Self::Failure) -> Option<i64> {
        None
    }

    fn archived(_order: &Order, _failure: &Self::Failure) -> Option<i64> {
        None
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
        thread::spawn(move || {
            let delivery = Delivery { id, via, tx: &send_tx, stop: &send_stop };
            deliver(&delivery, &transport, &received);
        });
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

struct Done {
    thread: Option<i64>,
    messages: Vec<i64>,
}

fn perform<T: Transport>(transport: &T, order: &Order, stop: &AtomicBool) -> Result<Done, T::Failure> {
    let mut attempt = T::Attempt::default();
    let sent = |done: Result<Option<i64>, T::Failure>, attempt: &T::Attempt| {
        done.map(|first| Done {
            thread: T::posted(attempt),
            messages: first.map_or_else(Vec::new, |first| [first].into_iter().chain(T::more(attempt)).collect()),
        })
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

struct Delivery<'a, E> {
    id: u64,
    via: Via,
    tx: &'a Sender<E>,
    stop: &'a AtomicBool,
}

impl<E: From<(Via, Event)>> Delivery<'_, E> {
    fn sent(&self, tag: u64, done: Done) -> bool {
        done.messages.into_iter().enumerate().all(|(i, message)| {
            let event = match done.thread {
                Some(thread) if i == 0 => Event::Posted { link: self.id, tag, thread, message },
                _ => Event::Sent { link: self.id, tag, message },
            };
            self.tell(event)
        })
    }

    fn failed<T: Transport>(&self, order: &Order, failure: &T::Failure) -> bool {
        if let Some(thread) = T::archived(order, failure) {
            log::debug!(self.via.target(), "the post is archived", method = T::method(order), thread = thread);
            return self.tell(Event::Archived { link: self.id, thread });
        }
        if let Some(thread) = T::gone(order, failure) {
            log::info!(self.via.target(), "the channel is gone", method = T::method(order), thread = thread);
            return self.tell(Event::Gone { link: self.id, thread, order: order.clone() });
        }
        log::warning!(self.via.target(), "not delivered", method = T::method(order), error = failure);
        order.tag().is_none_or(|tag| self.tell(Event::Failed { link: self.id, tag }))
    }

    fn tell(&self, event: Event) -> bool {
        self.tx.send((self.via, event).into()).is_ok()
    }
}

fn deliver<E: From<(Via, Event)>, T: Transport>(delivery: &Delivery<E>, transport: &T, orders: &Receiver<Order>) {
    for order in orders {
        if delivery.stop.load(Ordering::Relaxed) {
            return;
        }
        let done = panics::job(|| Ok(perform(transport, &order, delivery.stop)))
            .unwrap_or_else(|e| Err(T::Failure::from(e.to_string())));
        match done {
            Ok(done) => {
                if order.tag().is_some_and(|tag| !delivery.sent(tag, done)) {
                    return;
                }
            }
            Err(failure) => {
                if !delivery.failed::<T>(&order, &failure) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
