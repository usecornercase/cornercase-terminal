use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::{Bot, Failure};
use crate::log;
use crate::messenger::link::{Event, Link, MOST_WAIT, backoff, nap};
use crate::messenger::{Update, Via};
use crate::panics;

pub(crate) const POLL: Duration = Duration::from_secs(30);
const VIA: Via = Via::Telegram;

pub(crate) fn start<E: From<(Via, Event)> + Send + 'static>(
    id: u64,
    bot: &Bot,
    tx: &Sender<E>,
    wait: Duration,
    since: i64,
) -> Link {
    let poller = bot.clone();
    let poll_tx = tx.clone();
    Link::start(id, VIA, tx, bot.clone(), move |stop| poll(id, &poller, &poll_tx, stop, wait, since))
}

fn poll<E: From<(Via, Event)>>(id: u64, bot: &Bot, tx: &Sender<E>, stop: &AtomicBool, wait: Duration, since: i64) {
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
                    if tx.send((VIA, Event::Problem(id, None)).into()).is_err() {
                        return;
                    }
                }
                let (updates, next) = super::parse(&values);
                offset = next.unwrap_or(offset);
                let fresh: Vec<Update> = updates.into_iter().filter(|u| u.date().is_none_or(|d| d >= since)).collect();
                if !fresh.is_empty() && tx.send((VIA, Event::Updates(id, fresh)).into()).is_err() {
                    return;
                }
            }
            Err(Failure::Rejected) => {
                log::warning!("telegram", "token refused", link = id);
                let _ = tx.send((VIA, Event::Rejected(id)).into());
                return;
            }
            Err(Failure::Wait(seconds)) => nap(stop, Duration::from_secs(seconds.min(MOST_WAIT))),
            Err(failure) => {
                failed += 1;
                if failed.is_power_of_two() {
                    log::warning!("telegram", "cannot read messages", link = id, failed = failed, error = failure);
                }
                if tx.send((VIA, Event::Problem(id, Some(failure.to_string()))).into()).is_err() {
                    return;
                }
                nap(stop, backoff(failed));
            }
        }
    }
}

#[cfg(test)]
mod tests;
