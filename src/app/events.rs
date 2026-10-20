use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::App;
use crate::control::{AppState, Event, FlowRuns, Ids, Kind, Report, Snapshot, What};
use crate::log;
use crate::project::Hierarchy as _;

pub(super) const SNAPSHOT_EVERY: Duration = Duration::from_millis(500);
pub(super) const BELL_EVERY: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Streamed {
    Event(String),
    Snapshot(String),
    End,
}

#[derive(Default)]
pub(super) struct Events {
    subscribers: Vec<Subscriber>,
    bells: Bells,
    out: Vec<(u64, Streamed)>,
    pub(super) snapshots: Snapshots,
    pub(super) app: Snapshots<AppState>,
    pub(super) runs: Snapshots<FlowRuns>,
    flow_messages: Vec<(u64, u64)>,
}

pub(super) struct Snapshots<T = Report> {
    watchers: Vec<u64>,
    last: Option<T>,
    revision: u64,
    built: Option<Instant>,
    due: bool,
    changed: bool,
    read: Option<Instant>,
    refreshed: Option<u64>,
    every: Duration,
}

impl<T> Default for Snapshots<T> {
    fn default() -> Self {
        Self {
            watchers: Vec::new(),
            last: None,
            revision: 0,
            built: None,
            due: false,
            changed: false,
            read: None,
            refreshed: None,
            every: SNAPSHOT_EVERY,
        }
    }
}

impl<T: Clone + PartialEq + Serialize + Default> Snapshots<T> {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn mark_changed(&mut self) {
        self.changed = true;
    }

    #[cfg(test)]
    pub(super) fn set_every(&mut self, every: Duration) {
        self.every = every;
    }

    #[cfg(test)]
    pub(super) fn built(&self) -> Option<Instant> {
        self.built
    }

    #[cfg(test)]
    pub(super) fn set_built(&mut self, built: Instant) {
        self.built = Some(built);
    }

    fn take(&mut self, report: T, now: Instant) -> Option<Snapshot<T>> {
        self.built = Some(now);
        self.due = false;
        self.changed = false;
        if self.last.as_ref() == Some(&report) {
            return None;
        }
        self.revision += 1;
        self.last = Some(report.clone());
        Some(Snapshot { revision: self.revision, report })
    }

    fn offer(&mut self, report: T, now: Instant, out: &mut Vec<(u64, Streamed)>, stream: &str) {
        let Some(text) = self.take(report, now).and_then(|snapshot| snapshot.streamed()) else { return };
        log::debug!("control", "snapshot", stream = stream, revision = self.revision, bytes = text.len());
        out.extend(self.watchers.iter().map(|watcher| (*watcher, Streamed::Snapshot(text.clone()))));
    }

    fn join(&mut self, client: u64) -> Snapshot<T> {
        self.watchers.push(client);
        self.current().unwrap_or_default()
    }

    fn forget(&mut self, client: u64) {
        self.watchers.retain(|watcher| *watcher != client);
        if self.watchers.is_empty() {
            self.last = None;
            self.due = false;
        }
    }

    fn current(&self) -> Option<Snapshot<T>> {
        self.last.clone().map(|report| Snapshot { revision: self.revision, report })
    }

    fn waits(&self, now: Instant) -> bool {
        self.built.is_some_and(|at| now.saturating_duration_since(at) < self.every)
    }

    fn ready(&mut self, now: Instant) -> bool {
        if self.watchers.is_empty() {
            return false;
        }
        if self.waits(now) {
            self.due = true;
            return false;
        }
        true
    }

    pub(super) fn wake(&self, now: Instant, viewing: bool) -> Option<Duration> {
        let built = self.built.filter(|_| self.due && (viewing || !self.watchers.is_empty()))?;
        Some(self.every.saturating_sub(now.saturating_duration_since(built)))
    }
}

struct Ringing {
    ids: Ids,
    told: Instant,
    pending: u64,
}

#[derive(Default)]
struct Bells {
    panes: BTreeMap<u64, Ringing>,
}

impl Bells {
    fn ring(&mut self, pane: u64, ids: Ids, count: u64, now: Instant) -> Option<u64> {
        let Some(ringing) = self.panes.get_mut(&pane) else {
            self.panes.insert(pane, Ringing { ids, told: now, pending: 0 });
            return Some(count);
        };
        if now.saturating_duration_since(ringing.told) >= BELL_EVERY {
            ringing.told = now;
            return Some(count + std::mem::take(&mut ringing.pending));
        }
        ringing.pending += count;
        None
    }

    fn due(&mut self, now: Instant) -> Vec<(Ids, u64)> {
        let mut told = Vec::new();
        self.panes.retain(|_, ringing| {
            if now.saturating_duration_since(ringing.told) < BELL_EVERY {
                return true;
            }
            if ringing.pending == 0 {
                return false;
            }
            told.push((ringing.ids, std::mem::take(&mut ringing.pending)));
            ringing.told = now;
            true
        });
        told
    }

    fn wake(&self, now: Instant) -> Option<Duration> {
        self.panes
            .values()
            .filter(|ringing| ringing.pending > 0)
            .map(|ringing| BELL_EVERY.saturating_sub(now.saturating_duration_since(ringing.told)))
            .min()
    }
}

struct Subscriber {
    client: u64,
    panes: Option<Vec<u64>>,
    host: bool,
    notices: bool,
}

impl Subscriber {
    fn wants(&self, event: &Event) -> bool {
        (self.host || !event.host())
            && (self.notices || !event.notice())
            && self.panes.as_ref().is_none_or(|panes| event.about(panes))
    }
}

impl Events {
    pub(super) fn listening(&self) -> bool {
        !self.subscribers.is_empty()
    }

    pub(super) fn subscribe(&mut self, client: u64, panes: Vec<u64>, host: bool, notices: bool) {
        let panes = (!panes.is_empty()).then_some(panes);
        self.subscribers.push(Subscriber { client, panes, host, notices });
    }

    fn hosting(&self) -> bool {
        self.subscribers.iter().any(|subscriber| subscriber.host)
    }

    pub(super) fn bell_wake(&self, now: Instant) -> Option<Duration> {
        self.bells.wake(now)
    }

    pub(super) fn forget(&mut self, client: u64) {
        self.subscribers.retain(|subscriber| subscriber.client != client);
        if !self.hosting() {
            self.bells.panes.clear();
        }
        self.snapshots.forget(client);
        self.app.forget(client);
        self.runs.forget(client);
        self.flow_messages.retain(|(follower, _)| *follower != client);
    }

    pub(super) fn publish(&mut self, events: &[Event]) {
        let Self { subscribers, out, bells, .. } = self;
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
                bells.panes.remove(&pane);
                unwatch_pane(subscribers, pane);
            }
        }
        for ended in subscribers.extract_if(.., |subscriber| subscriber.panes.as_ref().is_some_and(Vec::is_empty)) {
            log::info!("control", "events ended", client = ended.client, reason = "every pane closed");
            out.push((ended.client, Streamed::End));
        }
    }
}

impl Events {
    pub(super) fn push(&mut self, client: u64, streamed: Streamed) {
        self.out.push((client, streamed));
    }

    pub(super) fn follows_flow_messages(&self, client: u64) -> bool {
        self.flow_messages.iter().any(|(follower, _)| *follower == client)
    }

    pub(super) fn follow_flow_messages(&mut self, client: u64, run: u64) {
        self.flow_messages.push((client, run));
    }

    pub(super) fn flow_message_followers(&self, run: u64) -> Vec<u64> {
        self.flow_messages.iter().filter(|(_, followed)| *followed == run).map(|(client, _)| *client).collect()
    }

    pub(super) fn stop_flow_messages(&mut self, running: impl Fn(u64) -> bool) -> Vec<u64> {
        self.flow_messages.extract_if(.., |(_, run)| !running(*run)).map(|(client, _)| client).collect()
    }
}

impl App {
    pub(super) fn tell_host(&mut self, pane: u64, copied: Vec<String>, rang: u32, now: Instant) {
        if !self.clients.events.hosting() || (copied.is_empty() && rang == 0) {
            return;
        }
        let Some((p, w, t)) = self.projects.tab_of_pane(pane) else { return };
        let workspace = &self.projects[p].workspaces[w];
        let ids = Ids {
            project: Some(self.projects[p].id),
            workspace: Some(workspace.id),
            tab: Some(workspace.tabs[t].id),
            pane: Some(pane),
        };
        let time = crate::time::Stamp(SystemTime::now()).to_string();
        let mut events: Vec<Event> = copied
            .into_iter()
            .map(|text| {
                log::debug!("control", "clipboard", pane = pane, bytes = text.len());
                Event { time: time.clone(), what: What::Clipboard { text }, ids }
            })
            .collect();
        if rang > 0
            && let Some(count) = self.clients.events.bells.ring(pane, ids, u64::from(rang), now)
        {
            events.push(Event { time, what: What::Bell { count }, ids });
        }
        self.clients.events.publish(&events);
    }

    pub(super) fn ring_bells(&mut self, now: Instant) {
        let due = self.clients.events.bells.due(now);
        if due.is_empty() {
            return;
        }
        let time = crate::time::Stamp(SystemTime::now()).to_string();
        let events: Vec<Event> =
            due.into_iter().map(|(ids, count)| Event { time: time.clone(), what: What::Bell { count }, ids }).collect();
        self.clients.events.publish(&events);
    }

    pub(crate) fn take_events(&mut self) -> Vec<(u64, Streamed)> {
        std::mem::take(&mut self.clients.events.out)
    }

    pub(super) fn watch_snapshots(&mut self, client: u64, now: Instant) -> Snapshot {
        let report = self.report(None);
        let Events { snapshots, out, .. } = &mut self.clients.events;
        snapshots.offer(report, now, out, "session");
        let snapshot = snapshots.join(client);
        log::info!("control", "watching", client = client, revision = snapshot.revision);
        snapshot
    }

    pub(super) fn app_snapshots(&mut self, client: u64, now: Instant) -> Snapshot<AppState> {
        let state = self.app_state();
        let Events { app, out, .. } = &mut self.clients.events;
        app.offer(state, now, out, "app");
        let snapshot = app.join(client);
        log::info!("control", "following the app", client = client, revision = snapshot.revision);
        snapshot
    }

    pub(super) fn publish_app(&mut self, now: Instant) {
        if self.clients.events.app.ready(now) {
            let state = self.app_state();
            let Events { app, out, .. } = &mut self.clients.events;
            app.offer(state, now, out, "app");
        }
    }

    pub(super) fn flow_runs_snapshots(&mut self, client: u64, now: Instant) -> Snapshot<FlowRuns> {
        let state = self.flow_runs_state();
        let Events { runs, out, .. } = &mut self.clients.events;
        runs.offer(state, now, out, "flow-runs");
        let snapshot = runs.join(client);
        log::info!("control", "following flow runs", client = client, revision = snapshot.revision);
        snapshot
    }

    pub(super) fn publish_flow_runs(&mut self, now: Instant) {
        if self.clients.events.runs.ready(now) {
            let state = self.flow_runs_state();
            let Events { runs, out, .. } = &mut self.clients.events;
            runs.offer(state, now, out, "flow-runs");
        }
    }

    pub(super) fn publish_snapshot(&mut self, now: Instant) {
        let watching = !self.clients.events.snapshots.watchers.is_empty();
        if !watching && self.clients.views.is_empty() {
            return;
        }
        if self.clients.events.snapshots.read != self.agents.watched {
            self.clients.events.snapshots.read = self.agents.watched;
            let refreshed = Some(self.refreshed());
            if self.clients.events.snapshots.refreshed != refreshed {
                self.clients.events.snapshots.refreshed = refreshed;
                self.clients.events.snapshots.changed = true;
            }
        }
        let own = self.clients.views.changed();
        let snapshots = &mut self.clients.events.snapshots;
        let paced = snapshots.waits(now);
        if !snapshots.changed && !own {
            return;
        }
        if paced && !own {
            snapshots.due = true;
            return;
        }
        let report = self.report(None);
        if paced {
            self.clients.events.snapshots.due = self.clients.events.snapshots.changed;
            self.publish_views(&report);
            return;
        }
        if watching {
            let Events { snapshots, out, .. } = &mut self.clients.events;
            snapshots.offer(report.clone(), now, out, "session");
        } else {
            let snapshots = &mut self.clients.events.snapshots;
            snapshots.built = Some(now);
            snapshots.due = false;
            snapshots.changed = false;
        }
        self.publish_views(&report);
    }
}

fn unwatch_pane(subscribers: &mut [Subscriber], pane: u64) {
    for panes in subscribers.iter_mut().filter_map(|subscriber| subscriber.panes.as_mut()) {
        panes.retain(|watched| *watched != pane);
    }
}
