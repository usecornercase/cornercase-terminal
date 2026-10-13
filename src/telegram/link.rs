use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::{Bot, Failure, Order, Update};
use crate::log;
use crate::panics;

pub(crate) const POLL: Duration = Duration::from_secs(30);
const NAP: Duration = Duration::from_millis(100);
const MOST_WAIT: u64 = 60;
const TRIES: usize = 3;
const NOT_MODIFIED: &str = "message is not modified";
const NO_ENTITIES: &str = "can't parse entities";

#[derive(Debug)]
pub(crate) enum Event {
    Updates(u64, Vec<Update>),
    Sent { link: u64, tag: u64, message: i64 },
    Problem(u64, Option<String>),
    Rejected(u64),
}

pub(crate) struct Link {
    pub(crate) id: u64,
    orders: Sender<Order>,
    stop: Arc<AtomicBool>,
}

impl Link {
    pub(crate) fn start<E: From<Event> + Send + 'static>(
        id: u64,
        bot: &Bot,
        tx: &Sender<E>,
        wait: Duration,
        since: i64,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (orders, received) = mpsc::channel();
        let (poller, sender) = (bot.clone(), bot.clone());
        let (poll_tx, send_tx) = (tx.clone(), tx.clone());
        let (poll_stop, send_stop) = (Arc::clone(&stop), Arc::clone(&stop));
        thread::spawn(move || poll(id, &poller, &poll_tx, &poll_stop, wait, since));
        thread::spawn(move || deliver(id, &sender, &received, &send_tx, &send_stop));
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

fn nap(stop: &AtomicBool, wait: Duration) {
    let until = Instant::now() + wait;
    while !stop.load(Ordering::Relaxed) && Instant::now() < until {
        thread::sleep(NAP.min(until.saturating_duration_since(Instant::now())));
    }
}

fn backoff(failed: u32) -> Duration {
    Duration::from_secs(1u64.checked_shl(failed.saturating_sub(1)).unwrap_or(MOST_WAIT).min(MOST_WAIT))
}

fn poll<E: From<Event>>(id: u64, bot: &Bot, tx: &Sender<E>, stop: &AtomicBool, wait: Duration, since: i64) {
    let mut offset = 0;
    let mut failed = 0u32;
    while !stop.load(Ordering::Relaxed) {
        let answer =
            panics::job(|| Ok(bot.updates(offset, wait))).unwrap_or_else(|e| Err(Failure::Unreachable(e.to_string())));
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match answer {
            Ok(values) => {
                if failed > 0 {
                    failed = 0;
                    log::info!("telegram", "reached again", link = id);
                    if tx.send(Event::Problem(id, None).into()).is_err() {
                        return;
                    }
                }
                let (updates, next) = super::parse(&values);
                offset = next.unwrap_or(offset);
                let fresh: Vec<Update> = updates.into_iter().filter(|u| u.date().is_none_or(|d| d >= since)).collect();
                if !fresh.is_empty() && tx.send(Event::Updates(id, fresh).into()).is_err() {
                    return;
                }
            }
            Err(Failure::Rejected) => {
                log::warning!("telegram", "token refused", link = id);
                let _ = tx.send(Event::Rejected(id).into());
                return;
            }
            Err(Failure::Wait(seconds)) => nap(stop, Duration::from_secs(seconds.min(MOST_WAIT))),
            Err(failure) => {
                failed += 1;
                if failed.is_power_of_two() {
                    log::warning!("telegram", "cannot read messages", link = id, failed = failed, error = failure);
                }
                if tx.send(Event::Problem(id, Some(failure.to_string())).into()).is_err() {
                    return;
                }
                nap(stop, backoff(failed));
            }
        }
    }
}

fn perform(bot: &Bot, order: &Order, stop: &AtomicBool) -> Result<Option<i64>, Failure> {
    let mut rich = true;
    for _ in 0..TRIES {
        match bot.perform(order, rich) {
            Err(Failure::Wait(seconds)) => nap(stop, Duration::from_secs(seconds.min(MOST_WAIT))),
            Err(Failure::Refused(why)) if rich && why.contains(NO_ENTITIES) => rich = false,
            Err(Failure::Refused(why)) if why.contains(NOT_MODIFIED) => return Ok(None),
            other => return other,
        }
    }
    bot.perform(order, rich)
}

fn deliver<E: From<Event>>(id: u64, bot: &Bot, orders: &Receiver<Order>, tx: &Sender<E>, stop: &AtomicBool) {
    for order in orders {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let done =
            panics::job(|| Ok(perform(bot, &order, stop))).unwrap_or_else(|e| Err(Failure::Unreachable(e.to_string())));
        match done {
            Ok(Some(message)) => {
                if let Some(tag) = order.tag()
                    && tx.send(Event::Sent { link: id, tag, message }.into()).is_err()
                {
                    return;
                }
            }
            Ok(None) => {}
            Err(failure) => log::warning!("telegram", "not delivered", method = order.method(), error = failure),
        }
    }
}

#[cfg(test)]
mod tests;
