use std::time::{Duration, Instant};

use super::App;
use crate::control::{Event, Kind, Report, Snapshot, What};
use crate::log;

pub(super) const SNAPSHOT_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Streamed {
    Event(String),
    Snapshot(String),
    End,
}

#[derive(Default)]
pub(super) struct Events {
    subscribers: Vec<Subscriber>,
    out: Vec<(u64, Streamed)>,
    pub(super) snapshots: Snapshots,
}

pub(super) struct Snapshots {
    watchers: Vec<u64>,
    last: Option<Report>,
    revision: u64,
    built: Option<Instant>,
    due: bool,
    pub(super) every: Duration,
}

impl Default for Snapshots {
    fn default() -> Self {
        Self { watchers: Vec::new(), last: None, revision: 0, built: None, due: false, every: SNAPSHOT_EVERY }
    }
}

impl Snapshots {
    fn take(&mut self, report: Report, now: Instant) -> Option<Snapshot> {
        self.built = Some(now);
        self.due = false;
        if self.last.as_ref() == Some(&report) {
            return None;
        }
        self.revision += 1;
        self.last = Some(report.clone());
        Some(Snapshot { revision: self.revision, report })
    }

    fn offer(&mut self, report: Report, now: Instant, out: &mut Vec<(u64, Streamed)>) {
        let Some(text) = self.take(report, now).and_then(|snapshot| snapshot.streamed()) else { return };
        log::debug!("control", "snapshot", revision = self.revision, bytes = text.len());
        out.extend(self.watchers.iter().map(|watcher| (*watcher, Streamed::Snapshot(text.clone()))));
    }

    fn current(&self) -> Option<Snapshot> {
        self.last.clone().map(|report| Snapshot { revision: self.revision, report })
    }

    fn waits(&self, now: Instant) -> bool {
        self.built.is_some_and(|at| now.saturating_duration_since(at) < self.every)
    }

    pub(super) fn wake(&self, now: Instant) -> Option<Duration> {
        let built = self.built.filter(|_| self.due && !self.watchers.is_empty())?;
        Some(self.every.saturating_sub(now.saturating_duration_since(built))).filter(|wait| !wait.is_zero())
    }
}

struct Subscriber {
    client: u64,
    panes: Option<Vec<u64>>,
}

impl Subscriber {
    fn wants(&self, event: &Event) -> bool {
        self.panes.as_ref().is_none_or(|panes| {
            event.kind() == Some(Kind::Pane) && event.ids.pane.is_some_and(|pane| panes.contains(&pane))
        })
    }
}

impl Events {
    pub(super) fn listening(&self) -> bool {
        !self.subscribers.is_empty()
    }

    pub(super) fn subscribe(&mut self, client: u64, panes: Vec<u64>) {
        let panes = (!panes.is_empty()).then_some(panes);
        self.subscribers.push(Subscriber { client, panes });
    }

    pub(super) fn forget(&mut self, client: u64) {
        self.subscribers.retain(|subscriber| subscriber.client != client);
        self.snapshots.watchers.retain(|watcher| *watcher != client);
        if self.snapshots.watchers.is_empty() {
            self.snapshots.last = None;
            self.snapshots.due = false;
        }
    }

    pub(super) fn publish(&mut self, events: &[Event]) {
        let Self { subscribers, out, .. } = self;
        if subscribers.is_empty() {
            return;
        }
        for event in events {
            let Some(text) = event.streamed() else { continue };
            for subscriber in subscribers.iter().filter(|subscriber| subscriber.wants(event)) {
                out.push((subscriber.client, Streamed::Event(text.clone())));
            }
            if event.what == (What::Closed { kind: Kind::Pane })
                && let Some(pane) = event.ids.pane
            {
                for panes in subscribers.iter_mut().filter_map(|subscriber| subscriber.panes.as_mut()) {
                    panes.retain(|watched| *watched != pane);
                }
            }
        }
        for ended in subscribers.extract_if(.., |subscriber| subscriber.panes.as_ref().is_some_and(Vec::is_empty)) {
            log::info!("control", "events ended", client = ended.client, reason = "every pane closed");
            out.push((ended.client, Streamed::End));
        }
    }
}

impl App {
    pub(crate) fn take_events(&mut self) -> Vec<(u64, Streamed)> {
        std::mem::take(&mut self.events.out)
    }

    pub(super) fn watch_snapshots(&mut self, client: u64, now: Instant) -> Snapshot {
        let report = self.report(None);
        let Events { snapshots, out, .. } = &mut self.events;
        snapshots.offer(report, now, out);
        snapshots.watchers.push(client);
        log::info!("control", "watching", client = client, revision = snapshots.revision);
        snapshots.current().unwrap_or_default()
    }

    pub(super) fn publish_snapshot(&mut self, now: Instant) {
        if self.events.snapshots.watchers.is_empty() {
            return;
        }
        if self.events.snapshots.waits(now) {
            self.events.snapshots.due = true;
            return;
        }
        let report = self.report(None);
        let Events { snapshots, out, .. } = &mut self.events;
        snapshots.offer(report, now, out);
    }
}
