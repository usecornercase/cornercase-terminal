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
    let poller = Poller { id, bot: bot.clone(), tx: tx.clone(), wait, since };
    Link::start(id, VIA, tx, bot.clone(), move |stop| poller.poll(stop))
}

struct Poller<E> {
    id: u64,
    bot: Bot,
    tx: Sender<E>,
    wait: Duration,
    since: i64,
}

impl<E: From<(Via, Event)>> Poller<E> {
    fn poll(&self, stop: &AtomicBool) {
        let mut offset = 0;
        let mut failed = 0u32;
        while !stop.load(Ordering::Relaxed) {
            let answer = panics::job(|| Ok(self.bot.updates(offset, self.wait)))
                .unwrap_or_else(|e| Err(Failure::Unreachable(e.to_string())));
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let go_on = match answer {
                Ok(values) => self.read(&values, &mut offset, &mut failed),
                Err(Failure::Rejected) => {
                    log::warning!("telegram", "token refused", link = self.id);
                    let _ = self.tx.send((VIA, Event::Rejected(self.id)).into());
                    false
                }
                Err(Failure::Wait(seconds)) => {
                    nap(stop, Duration::from_secs(seconds.min(MOST_WAIT)));
                    true
                }
                Err(failure) => self.fail(&failure, &mut failed, stop),
            };
            if !go_on {
                return;
            }
        }
    }

    fn read(&self, values: &[serde_json::Value], offset: &mut i64, failed: &mut u32) -> bool {
        if *failed > 0 {
            *failed = 0;
            log::info!("telegram", "reached again", link = self.id);
            if self.tx.send((VIA, Event::Problem(self.id, None)).into()).is_err() {
                return false;
            }
        }
        let (updates, next) = super::parse(values);
        *offset = next.unwrap_or(*offset);
        let fresh: Vec<Update> = updates.into_iter().filter(|u| u.date().is_none_or(|d| d >= self.since)).collect();
        fresh.is_empty() || self.tx.send((VIA, Event::Updates(self.id, fresh)).into()).is_ok()
    }

    fn fail(&self, failure: &Failure, failed: &mut u32, stop: &AtomicBool) -> bool {
        *failed += 1;
        if failed.is_power_of_two() {
            log::warning!("telegram", "cannot read messages", link = self.id, failed = *failed, error = failure);
        }
        if self.tx.send((VIA, Event::Problem(self.id, Some(failure.to_string()))).into()).is_err() {
            return false;
        }
        nap(stop, backoff(*failed));
        true
    }
}

#[cfg(test)]
mod tests;
