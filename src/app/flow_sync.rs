use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::App;
use super::control::machines::unknown;
use super::control::{Handled, conflict, json};
use super::event::AppEvent;
use super::machine_links::MachineEvent;
use super::toast::Toast;
use crate::control::contract::{Code, Refusal};
use crate::control::{FlowSpec, FlowSync, FlowsSynced, MachineFlows};
use crate::flows::dialog::SyncShown;
use crate::flows::store::{self, Origin};
use crate::flows::sync::{self, Change, Journal, Ours};
use crate::flows::{Flow, share};
use crate::host;
use crate::log::{self, Job, Level};
use crate::machines::flows::{self as sending, Sending, Synced};
use crate::machines::{self, State};
use crate::paths;
use crate::time::Stamp;
use crate::ui;

pub(super) const NONE_SYNC: &str = "no machine syncs your flows: turn on `sync flows` in Settings › Machines, or `cornercase machines edit NAME --sync-flows`";

#[derive(Default)]
pub(super) struct Syncing {
    peers: Vec<Peer>,
    waiters: Vec<Waiter>,
}

struct Peer {
    destination: String,
    linked: bool,
    running: bool,
    again: bool,
    rounds: u64,
    same: Vec<(String, Option<SystemTime>)>,
    last: Option<MachineFlows>,
}

struct Waiter {
    key: u64,
    wanted: Vec<(String, String, u64)>,
    reports: Vec<MachineFlows>,
}

impl Syncing {
    #[cfg(test)]
    pub(super) fn rounds(&self, destination: &str) -> u64 {
        self.peer(destination).map_or(0, |peer| peer.rounds)
    }

    fn peer(&self, destination: &str) -> Option<&Peer> {
        self.peers.iter().find(|peer| peer.destination == destination)
    }

    fn peer_mut(&mut self, destination: &str) -> Option<&mut Peer> {
        self.peers.iter_mut().find(|peer| peer.destination == destination)
    }

    fn reached(&self, waiter: &Waiter) -> bool {
        waiter.wanted.iter().all(|(destination, _, round)| self.peer(destination).is_none_or(|p| p.rounds >= *round))
    }

    fn report(&self, destination: &str, name: &str) -> MachineFlows {
        let gone = || MachineFlows {
            machine: name.into(),
            error: Some(format!("{name} no longer syncs your flows")),
            ..MachineFlows::default()
        };
        self.peer(destination).and_then(|peer| peer.last.clone()).unwrap_or_else(gone)
    }
}

pub(super) fn stamp_now() -> String {
    Stamp(SystemTime::now()).to_string()
}

fn syncing_machines(machines: &[machines::Machine]) -> impl Iterator<Item = &machines::Machine> {
    machines::linked(machines).filter(|machine| machine.sync_flows)
}

impl App {
    pub(super) fn yours_flows(&self) -> PathBuf {
        paths::flows(self.config_file.path())
    }

    fn flow_journal(&self) -> PathBuf {
        sync::path(self.config_file.path())
    }

    pub(super) fn stamped(&self, dir: &Path, flow: &Flow) -> Flow {
        let changed = (dir == self.yours_flows()).then(stamp_now);
        Flow { changed, ..flow.clone() }
    }

    pub(super) fn journal_flow(&self, name: &str, change: Option<Change>) {
        let path = self.flow_journal();
        let mut journal = Journal::read(&path);
        let changed = match change {
            Some(change) => {
                journal.record(name, change, &stamp_now());
                true
            }
            None => journal.forget(name),
        };
        if changed && let Err(e) = journal.write(&path) {
            log::warning!("flows", "the flows' sync journal could not be saved", why = e);
        }
    }

    pub(super) fn journal_saved(&mut self, dir: &Path, flow: &Flow, previous: Option<(&Path, &str)>) {
        let yours = self.yours_flows();
        let left = previous.filter(|(path, old)| {
            path.parent() == Some(&yours) && (dir != yours || !store::same_name(old, &flow.name))
        });
        if let Some((_, old)) = left {
            self.journal_flow(old, Some(Change::Deleted));
        }
        if dir == yours {
            self.journal_flow(&flow.name, None);
        }
        if left.is_some() || dir == yours {
            self.flows_changed_here();
        }
    }

    pub(super) fn save_imported(
        &mut self,
        dir: &Path,
        flow: &Flow,
        replaced: Option<&Path>,
        from: Option<&str>,
    ) -> Result<PathBuf, String> {
        let yours = dir == self.yours_flows();
        let before = replaced.and_then(|path| store::read(path).ok());
        let flow = if from.is_some() && yours { flow.clone() } else { self.stamped(dir, flow) };
        let saved = store::save(dir, &flow, replaced).map_err(|e| format!("could not save the flow: {e}"))?;
        if yours {
            self.journal_flow(&flow.name, None);
            let differs = before.is_some_and(|before| FlowSpec::from(&before) != FlowSpec::from(&flow));
            if let Some(from) = from.filter(|_| differs) {
                self.replaced_from(&flow.name, from);
            }
            self.flows_changed_here();
        }
        Ok(saved)
    }

    fn replaced_from(&mut self, name: &str, from: &str) {
        let from = if from.trim().is_empty() { "another machine".to_string() } else { format!("{}'s", from.trim()) };
        let name = share::visible(name.trim());
        log::info!("flows", "flow replaced by a sync", name = name);
        let message = format!("{name} was replaced by {from} copy: your flows sync from there");
        let mut toast = Toast::new(message, ui::ToastIcon::Check);
        toast.lingers = true;
        self.messages.toast = Some(toast);
    }

    pub(super) fn journal_deleted(&mut self, path: &Path, name: Option<&str>) {
        if path.parent() != Some(&self.yours_flows()) {
            return;
        }
        if let Some(name) = name {
            self.journal_flow(name, Some(Change::Deleted));
        }
        self.flows_changed_here();
    }

    pub(super) fn sync_shown(&self, listing: &store::Listing) -> SyncShown {
        let synced = listing
            .flows
            .iter()
            .filter(|saved| matches!(saved.origin, Origin::User(_)))
            .map(|saved| (saved.flow.name.clone(), self.flow_synced_to(&saved.flow)))
            .filter(|(_, machines)| !machines.is_empty())
            .collect();
        SyncShown { machines: self.flow_sync_machines(), synced }
    }

    pub(super) fn flows_sync_now(&mut self) {
        let started = self.sync_flows_now();
        let notice = if self.flow_sync_machines().is_empty() {
            NONE_SYNC.to_string()
        } else if started.is_empty() {
            "no machine that syncs your flows is linked now: they get them when their link comes back".to_string()
        } else {
            format!("sending your flows to {}…", started.join(", "))
        };
        self.flows_notice(notice);
    }

    pub(super) fn flows_changed_here(&mut self) {
        let linked: Vec<String> =
            self.machines.flows.peers.iter().filter(|peer| peer.linked).map(|peer| peer.destination.clone()).collect();
        for destination in linked {
            self.start_flow_sync(&destination);
        }
    }

    pub(super) fn follow_flow_sync(&mut self) {
        let wanted: Vec<(String, bool)> = syncing_machines(&self.config.machines)
            .map(|machine| (machine.destination.clone(), self.linked_destination(&machine.destination)))
            .collect();
        let syncing = &mut self.machines.flows;
        syncing.peers.retain(|peer| wanted.iter().any(|(destination, _)| *destination == peer.destination));
        let mut rose = Vec::new();
        for (destination, linked) in wanted {
            if syncing.peer(&destination).is_none() {
                syncing.peers.push(Peer {
                    destination: destination.clone(),
                    linked: false,
                    running: false,
                    again: false,
                    rounds: 0,
                    same: Vec::new(),
                    last: None,
                });
            }
            let Some(peer) = syncing.peer_mut(&destination) else { continue };
            if linked && !peer.linked {
                rose.push(destination);
            }
            peer.linked = linked;
        }
        for destination in rose {
            self.start_flow_sync(&destination);
        }
        self.answer_flow_waiters();
    }

    fn linked_destination(&self, destination: &str) -> bool {
        self.machines.held().iter().any(|held| held.destination == destination && held.status.state == State::Linked)
    }

    fn start_flow_sync(&mut self, destination: &str) -> Option<u64> {
        let held = self.machines.held().iter().find(|held| held.destination == destination)?;
        if held.status.state != State::Linked {
            return None;
        }
        let name = held.name.clone();
        let peer = self.machines.flows.peer_mut(destination)?;
        if peer.running {
            peer.again = true;
            return Some(peer.rounds + 2);
        }
        peer.running = true;
        let round = peer.rounds + 1;
        let (ours, texts) = self.flows_to_send(destination);
        let connect = self.machines.connector(destination);
        let there = destination.to_string();
        let job = Job::new(Level::Info, "flows", "sync").with("machine", &name);
        let asked = name.clone();
        let work = move || {
            let sending = Sending { ours, texts, from: host::name::computer() };
            Ok(sending::sync(connect.as_ref(), &asked, &sending))
        };
        self.spawn_job(job, work, move |result, _| {
            let synced = result.unwrap_or_else(|error| Synced {
                report: MachineFlows { machine: name, error: Some(error.to_string()), ..MachineFlows::default() },
                ..Synced::default()
            });
            Some(AppEvent::Machine(MachineEvent::FlowsSynced { destination: there, synced: Box::new(synced) }))
        });
        Some(round)
    }

    fn flows_to_send(&self, destination: &str) -> (Ours<FlowSpec>, Vec<(String, String)>) {
        let listing = self.flow_listing(None);
        let journal = Journal::read(&self.flow_journal());
        let mut flows = Vec::new();
        let mut texts = Vec::new();
        for saved in listing.flows.iter().filter(|saved| matches!(saved.origin, Origin::User(_))) {
            let Ok(text) = share::to_share(&saved.flow) else { continue };
            let changed = saved.flow.changed.as_deref().and_then(Stamp::parse);
            flows.push(sync::Held { name: saved.flow.name.clone(), changed, spec: FlowSpec::from(&saved.flow) });
            texts.push((saved.flow.name.clone(), text));
        }
        let synced = journal.synced(destination);
        let ours = Ours { flows, hidden: self.config.hidden_flows.clone(), events: journal.events, synced };
        (ours, texts)
    }

    pub(super) fn flows_synced(&mut self, destination: &str, synced: Synced) {
        let report = hidden_report(synced.report, destination);
        log_report(&report);
        if let Some(started) = synced.started {
            let path = self.flow_journal();
            let mut journal = Journal::read(&path);
            journal.synced.insert(destination.to_string(), Stamp(started).to_string());
            if let Err(e) = journal.write(&path) {
                log::warning!("flows", "the flows' sync journal could not be saved", why = e);
            }
        }
        let Some(peer) = self.machines.flows.peer_mut(destination) else { return self.answer_flow_waiters() };
        peer.running = false;
        peer.rounds += 1;
        if synced.started.is_some() {
            peer.same = synced.same;
        }
        peer.last = Some(report);
        let again = std::mem::take(&mut peer.again);
        if again {
            self.start_flow_sync(destination);
        }
        self.answer_flow_waiters();
        self.refresh_flows();
    }

    fn answer_flow_waiters(&mut self) {
        let waiters = std::mem::take(&mut self.machines.flows.waiters);
        for waiter in waiters {
            if !self.machines.flows.reached(&waiter) {
                self.machines.flows.waiters.push(waiter);
                continue;
            }
            let mut machines = waiter.reports;
            machines.extend(
                waiter.wanted.iter().map(|(destination, name, _)| self.machines.flows.report(destination, name)),
            );
            self.answer_read(waiter.key, move |_| Ok(json(&FlowsSynced { machines })));
        }
    }

    pub(super) fn flow_synced_to(&self, flow: &Flow) -> Vec<String> {
        let changed = flow.changed.as_deref().and_then(Stamp::parse);
        let held =
            |peer: &Peer| peer.same.iter().any(|(name, at)| store::same_name(name, &flow.name) && *at == changed);
        syncing_machines(&self.config.machines)
            .filter(|machine| self.machines.flows.peer(&machine.destination).is_some_and(held))
            .map(|machine| machine.name.clone())
            .collect()
    }

    pub(super) fn flow_sync_machines(&self) -> Vec<String> {
        syncing_machines(&self.config.machines).map(|machine| machine.name.clone()).collect()
    }

    pub(super) fn sync_flows_now(&mut self) -> Vec<String> {
        self.follow_machines();
        self.follow_flow_sync();
        let targets: Vec<(String, String)> = syncing_machines(&self.config.machines)
            .map(|machine| (machine.name.clone(), machine.destination.clone()))
            .collect();
        targets
            .into_iter()
            .filter(|(_, destination)| self.start_flow_sync(destination).is_some())
            .map(|(name, _)| name)
            .collect()
    }

    pub(super) fn flow_sync_request(&mut self, client: u64, request: &FlowSync) -> Handled {
        self.follow_machines();
        self.follow_flow_sync();
        let targets = self.flow_sync_targets(request.machine.as_deref())?;
        let key = self.clients.requests.key();
        let mut waiter = Waiter { key, wanted: Vec::new(), reports: Vec::new() };
        for (name, destination) in targets {
            if let Some(round) = self.start_flow_sync(&destination) {
                waiter.wanted.push((destination, name, round));
            } else {
                let error = Some(self.not_linked(&name, &destination));
                waiter.reports.push(MachineFlows { machine: name, error, ..MachineFlows::default() });
            }
        }
        if waiter.wanted.is_empty() {
            return Ok(Some(json(&FlowsSynced { machines: waiter.reports })));
        }
        self.machines.flows.waiters.push(waiter);
        self.pend_reading(client, key);
        Ok(None)
    }

    fn flow_sync_targets(&self, machine: Option<&str>) -> Result<Vec<(String, String)>, Refusal> {
        let all: Vec<(String, String)> = syncing_machines(&self.config.machines)
            .map(|machine| (machine.name.clone(), machine.destination.clone()))
            .collect();
        let Some(asked) = machine.map(str::trim) else {
            return if all.is_empty() { Err(conflict(NONE_SYNC)) } else { Ok(all) };
        };
        let listed = self.config.machines.iter().find(|listed| listed.name.eq_ignore_ascii_case(asked));
        let Some(listed) = listed else { return Err(unknown(asked)) };
        if !listed.sync_flows {
            let name = &listed.name;
            return Err(conflict(format!(
                "{name} does not sync your flows: turn on `sync flows` in its form in Settings › Machines, or `cornercase machines edit {name} --sync-flows`"
            )));
        }
        let found = all.into_iter().find(|(name, _)| name.eq_ignore_ascii_case(asked));
        let name = &listed.name;
        let why = || {
            if listed.shown {
                format!("`{name}` has no link until it is fixed: `cornercase machines` says why")
            } else {
                format!("`{name}` is hidden, so it has no link: `cornercase machines show {name}` links it again")
            }
        };
        found.map(|found| vec![found]).ok_or_else(|| Refusal::new(Code::NotFound, why()))
    }

    fn not_linked(&self, name: &str, destination: &str) -> String {
        match self.machines.held().iter().find(|held| held.destination == destination) {
            Some(held) if held.status.state == State::Connecting => format!("{name} is still connecting"),
            Some(held) => self.unlinked(held),
            None => format!("{name} is still connecting"),
        }
    }
}

fn hidden_report(mut report: MachineFlows, destination: &str) -> MachineFlows {
    let name = report.machine.clone();
    let hide = |text: &str| machines::hide_destination(text, &name, destination);
    report.error = report.error.as_deref().map(hide);
    for failed in &mut report.failed {
        failed.error = hide(&failed.error);
    }
    report
}

fn log_report(report: &MachineFlows) {
    let machine = &report.machine;
    let changes = [
        ("sent", &report.sent),
        ("deleted", &report.deleted),
        ("hidden", &report.hidden),
        ("restored", &report.restored),
    ];
    for (change, names) in changes {
        for name in names {
            log::info!("flows", "flow synced", machine = machine, name = name, change = change);
        }
    }
    for failed in &report.failed {
        log::warning!("flows", "flow not synced", machine = machine, name = failed.name, why = failed.error);
    }
    if let Some(error) = &report.error {
        log::warning!("flows", "flows not synced", machine = machine, why = error);
    }
}
