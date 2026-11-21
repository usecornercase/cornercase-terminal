use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::App;
use super::control::machines::unknown;
use super::control::{Handled, conflict, json};
use super::event::AppEvent;
use super::machine_links::MachineEvent;
use super::toast::Toast;
use crate::control::contract::{Code, Refusal};
use crate::control::{FlowSpec, FlowSync, FlowSyncFailure, FlowsSynced, MachineFlows};
use crate::flows::store::{self, Folder, Listing, Origin, Saved};
use crate::flows::sync::{self, Held};
use crate::flows::{Flow, share};
use crate::host;
use crate::log::{self, Job, Level};
use crate::machines::flows::{self as sending, Sending};
use crate::machines::{self, State};
use crate::paths;
use crate::time::Stamp;
use crate::ui;

const NONE_LINKED: &str =
    "no machine to sync your flows to: add one in Settings › Machines, or `cornercase machines add`";

#[derive(Default)]
pub(super) struct Syncing {
    waiters: Vec<Waiter>,
    next: u64,
}

enum Answer {
    Request(u64),
    Window(String),
}

struct Waiter {
    id: u64,
    answer: Answer,
    pending: usize,
    reports: Vec<MachineFlows>,
    left_out: Vec<FlowSyncFailure>,
}

struct Started {
    id: u64,
    machines: Vec<String>,
    reports: Vec<MachineFlows>,
    left_out: Vec<FlowSyncFailure>,
}

impl Syncing {
    #[cfg(all(test, unix))]
    pub(super) fn waiting(&self) -> usize {
        self.waiters.len()
    }

    fn wait(&mut self, started: Started, answer: Answer) {
        if matches!(answer, Answer::Window(_)) {
            self.waiters.retain(|waiter| !matches!(waiter.answer, Answer::Window(_)));
        }
        let Started { id, machines, reports, left_out } = started;
        self.waiters.push(Waiter { id, answer, pending: machines.len(), reports, left_out });
    }

    fn settled(&mut self, id: u64, mut report: MachineFlows) -> Option<Waiter> {
        let at = self.waiters.iter().position(|waiter| waiter.id == id)?;
        let waiter = &mut self.waiters[at];
        report.failed.splice(0..0, waiter.left_out.iter().cloned());
        waiter.reports.push(report);
        waiter.pending = waiter.pending.saturating_sub(1);
        (waiter.pending == 0).then(|| self.waiters.remove(at))
    }
}

pub(super) fn stamp_now() -> String {
    Stamp(SystemTime::now()).to_string()
}

impl App {
    pub(super) fn yours_flows(&self) -> PathBuf {
        paths::flows(self.config_file.path())
    }

    pub(super) fn folder_of(&self, dir: &Path) -> Folder {
        let project = self.projects.iter().any(|project| dir == store::project_dir(&project.path));
        if project && dir != self.yours_flows() { Folder::Project } else { Folder::Yours }
    }

    pub(super) fn stamped(&self, dir: &Path, flow: &Flow) -> Flow {
        let changed = (dir == self.yours_flows()).then(stamp_now);
        Flow { changed, ..flow.clone() }
    }

    pub(super) fn flow_sync_machines(&self) -> Vec<String> {
        machines::linked(&self.config.machines).map(|machine| machine.name.clone()).collect()
    }

    pub(super) fn follow_flow_sync_machines(&mut self) {
        let names = self.flow_sync_machines();
        if let Some(dialog) = self.flows_dialog() {
            dialog.sync_to = names;
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
        let synced = from.filter(|_| yours);
        let flow = if synced.is_some() { kept_time(flow) } else { self.stamped(dir, flow) };
        if synced.is_some() && before.as_ref().is_some_and(|before| newer_than(before, &flow)) {
            return Err(sync::NEWER_HERE.into());
        }
        let saved = store::save_in(dir, &flow, replaced, self.folder_of(dir))
            .map_err(|e| format!("could not save the flow: {e}"))?;
        let arrived = match &before {
            Some(before) => (FlowSpec::from(before) != FlowSpec::from(&flow)).then_some(Arrival::Replaced),
            None => Some(Arrival::New),
        };
        if let (Some(from), Some(arrived)) = (synced, arrived) {
            self.arrived_from(&flow.name, from, arrived);
        }
        Ok(saved)
    }

    fn arrived_from(&mut self, name: &str, from: &str, arrived: Arrival) {
        let from = sending::sender(from);
        let whose = if from.is_empty() { "another machine's".to_string() } else { format!("{from}'s") };
        let name = share::visible(name.trim());
        let message = match arrived {
            Arrival::Replaced => {
                log::info!("flows", "flow replaced by a sync", name = name, from = from);
                format!("{name} was replaced by {whose} copy: it synced its flows here")
            }
            Arrival::New => {
                log::info!("flows", "flow added by a sync", name = name, from = from);
                format!("{name} arrived from {whose} flows: it synced them here")
            }
        };
        let mut toast = Toast::new(message, ui::ToastIcon::Check);
        toast.lingers = true;
        self.messages.toast = Some(toast);
    }

    pub(super) fn flows_sync_now(&mut self, name: Option<String>) {
        let request = FlowSync { machine: None, name };
        let started = match self.start_flow_sync(&request) {
            Ok(started) => started,
            Err(refusal) => return self.flows_notice(refusal.message),
        };
        if started.machines.is_empty() {
            return self.flows_notice(outcome(&started.reports));
        }
        let what = request.name.as_deref().map_or_else(|| "your flows".into(), |name| share::visible(name.trim()));
        let notice = format!("sending {what} to {}…", started.machines.join(", "));
        self.machines.flows.wait(started, Answer::Window(notice.clone()));
        self.flows_notice(notice);
    }

    pub(super) fn flow_sync_request(&mut self, client: u64, request: &FlowSync) -> Handled {
        let started = self.start_flow_sync(request)?;
        if started.machines.is_empty() {
            return Ok(Some(json(&FlowsSynced { machines: started.reports })));
        }
        let key = self.clients.requests.key();
        self.machines.flows.wait(started, Answer::Request(key));
        self.pend_reading(client, key);
        Ok(None)
    }

    fn start_flow_sync(&mut self, request: &FlowSync) -> Result<Started, Refusal> {
        self.follow_machines();
        let targets = self.flow_sync_targets(request.machine.as_deref())?;
        let outgoing = self.flows_to_send(request.name.as_deref())?;
        self.machines.flows.next += 1;
        let left_out = outgoing.left_out.clone();
        let mut started = Started { id: self.machines.flows.next, machines: Vec::new(), reports: Vec::new(), left_out };
        for failed in &started.left_out {
            log::warning!("flows", "flow left out of a sync", name = failed.name);
        }
        for (name, destination) in targets {
            if self.linked_destination(&destination) {
                self.send_flows(started.id, &name, &destination, outgoing.clone());
                started.machines.push(name);
            } else {
                let error = Some(self.not_linked(&name, &destination));
                let failed = outgoing.left_out.clone();
                started.reports.push(MachineFlows { machine: name, error, failed, ..MachineFlows::default() });
            }
        }
        Ok(started)
    }

    fn send_flows(&self, waiter: u64, name: &str, destination: &str, outgoing: Sending) {
        let connect = self.machines.connector(destination);
        let job = Job::new(Level::Info, "flows", "sync").with("machine", name);
        let (asked, machine, there) = (name.to_string(), name.to_string(), destination.to_string());
        let work = move || {
            let outgoing = Sending { from: host::name::computer(), ..outgoing };
            Ok(sending::sync(connect.as_ref(), &asked, &outgoing))
        };
        self.spawn_job(job, work, move |result, _| {
            let report = result.unwrap_or_else(|error| MachineFlows {
                machine,
                error: Some(error.to_string()),
                ..MachineFlows::default()
            });
            Some(AppEvent::Machine(MachineEvent::FlowsSynced { waiter, destination: there, report: Box::new(report) }))
        });
    }

    pub(super) fn flows_synced(&mut self, waiter: u64, destination: &str, report: MachineFlows) {
        let report = hidden_report(report, destination);
        log_report(&report);
        let Some(mut waiter) = self.machines.flows.settled(waiter, report) else { return };
        let order = |report: &MachineFlows| self.config.machines.iter().position(|m| m.name == report.machine);
        waiter.reports.sort_by_key(order);
        let machines = waiter.reports;
        match waiter.answer {
            Answer::Request(key) => self.answer_read(key, move |_| Ok(json(&FlowsSynced { machines }))),
            Answer::Window(sending) => self.flows_sync_shown(&sending, &machines),
        }
    }

    fn flows_sync_shown(&mut self, sending: &str, machines: &[MachineFlows]) {
        if let Some(dialog) = self.flows_dialog()
            && dialog.notice.as_deref() == Some(sending)
        {
            dialog.notice = Some(outcome(machines));
        }
    }

    pub(super) fn flows_to_send(&self, name: Option<&str>) -> Result<Sending, Refusal> {
        let listing = self.flow_listing(None);
        let wanted = |saved: &&Saved| name.is_none_or(|name| store::same_name(&saved.flow.name, name));
        let yours: Vec<&Saved> =
            listing.flows.iter().filter(|saved| matches!(saved.origin, Origin::User(_))).filter(wanted).collect();
        let unreadable = self.unreadable_flows(&listing, name.is_none());
        if let Some(name) = name
            && yours.is_empty()
            && unreadable.is_empty()
        {
            return Err(not_yours(&listing, name));
        }
        let now = SystemTime::now();
        let mut outgoing = Sending::default();
        for copies in by_name(yours) {
            match copies.as_slice() {
                [saved] => add_flow(&mut outgoing, saved, now),
                _ => outgoing.left_out.push(duplicated(&copies)),
            }
        }
        outgoing.left_out.extend(unreadable);
        Ok(outgoing)
    }

    fn unreadable_flows(&self, listing: &Listing, every: bool) -> Vec<FlowSyncFailure> {
        let dir = self.yours_flows();
        let folder = store::unreadable(&dir).map(|why| FlowSyncFailure {
            name: dir.display().to_string(),
            error: format!("the folder of your flows could not be read: {why}"),
        });
        let files = listing.broken.iter().filter(|(path, _)| every && path.parent() == Some(dir.as_path()));
        let files = files.map(|(path, why)| FlowSyncFailure { name: file_name(path), error: why.clone() });
        folder.into_iter().chain(files).collect()
    }

    fn flow_sync_targets(&self, machine: Option<&str>) -> Result<Vec<(String, String)>, Refusal> {
        let all: Vec<(String, String)> = machines::linked(&self.config.machines)
            .map(|machine| (machine.name.clone(), machine.destination.clone()))
            .collect();
        let Some(asked) = machine.map(str::trim) else {
            return if all.is_empty() { Err(conflict(self.none_linked())) } else { Ok(all) };
        };
        let listed = self.config.machines.iter().find(|listed| listed.name.eq_ignore_ascii_case(asked));
        let Some(listed) = listed else { return Err(unknown(asked)) };
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

    fn none_linked(&self) -> String {
        match machines::accepted(&self.config.machines).find(|machine| !machine.shown) {
            Some(hidden) => format!(
                "no machine is shown here to sync your flows to: `cornercase machines show {}` links it again",
                hidden.name
            ),
            None => NONE_LINKED.into(),
        }
    }

    fn linked_destination(&self, destination: &str) -> bool {
        self.machines.held().iter().any(|held| held.destination == destination && held.status.state == State::Linked)
    }

    fn not_linked(&self, name: &str, destination: &str) -> String {
        match self.machines.held().iter().find(|held| held.destination == destination) {
            Some(held) if held.status.state == State::Connecting => format!("{name} is still connecting"),
            Some(held) => self.unlinked(held),
            None => format!("{name} is still connecting"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrival {
    New,
    Replaced,
}

fn changed_at(flow: &Flow) -> Option<SystemTime> {
    flow.changed.as_deref().and_then(Stamp::parse)
}

fn newer_than(here: &Flow, arriving: &Flow) -> bool {
    sync::newer(changed_at(here), changed_at(arriving))
}

fn not_yours(listing: &Listing, name: &str) -> Refusal {
    let shown = share::visible(name.trim());
    let built_in = listing.flows.iter().any(|saved| store::same_name(&saved.flow.name, name));
    if built_in {
        return conflict(format!(
            "{shown} is a built-in flow you did not change: only your own flows sync, edited built-ins included"
        ));
    }
    Refusal::new(Code::NotFound, format!("no flow of yours is named {shown}: `cornercase flow list` lists them"))
}

fn add_flow(outgoing: &mut Sending, saved: &Saved, now: SystemTime) {
    let name = saved.flow.name.clone();
    let text = in_the_present(saved, now).and_then(|flow| share::to_share(&flow).map(|text| (flow, text)));
    match text {
        Ok((flow, text)) => {
            outgoing.flows.push(Held { name: name.clone(), changed: changed_at(&flow), spec: FlowSpec::from(&flow) });
            outgoing.texts.push((name, text));
        }
        Err(error) => outgoing.left_out.push(FlowSyncFailure { name, error }),
    }
}

fn present(flow: &Flow, now: SystemTime) -> Option<Flow> {
    let future = flow.changed.as_deref().is_some_and(|changed| sync::in_the_future(changed, now));
    future.then(|| Flow { changed: Some(Stamp(now).to_string()), ..flow.clone() })
}

fn kept_time(flow: &Flow) -> Flow {
    let Some(fixed) = present(flow, SystemTime::now()) else { return flow.clone() };
    log::warning!("flows", "a synced flow's time is in the future: it is saved with this computer's time");
    fixed
}

fn by_name(yours: Vec<&Saved>) -> Vec<Vec<&Saved>> {
    let mut named: Vec<Vec<&Saved>> = Vec::new();
    for saved in yours {
        match named.iter_mut().find(|copies| store::same_name(&copies[0].flow.name, &saved.flow.name)) {
            Some(copies) => copies.push(saved),
            None => named.push(vec![saved]),
        }
    }
    named
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn duplicated(copies: &[&Saved]) -> FlowSyncFailure {
    let files: Vec<String> = copies.iter().filter_map(|saved| saved.origin.path()).map(file_name).collect();
    FlowSyncFailure {
        name: copies[0].flow.name.clone(),
        error: format!(
            "{} files of yours have this name ({}): rename or delete one, then sync again",
            copies.len(),
            files.join(", ")
        ),
    }
}

fn in_the_present(saved: &Saved, now: SystemTime) -> Result<Flow, String> {
    let Some(fixed) = present(&saved.flow, now) else { return Ok(saved.flow.clone()) };
    let ahead = "its time is ahead of this computer's clock and could not be fixed";
    let path = saved.origin.path().ok_or_else(|| ahead.to_string())?;
    let dir = path.parent().unwrap_or(path);
    store::save_in(dir, &fixed, Some(path), Folder::Yours).map_err(|e| format!("{ahead}: {e}"))?;
    log::warning!("flows", "a flow's time is in the future: it is saved with this computer's time");
    Ok(fixed)
}

fn outcome(machines: &[MachineFlows]) -> String {
    machines.iter().map(MachineFlows::line).collect::<Vec<_>>().join("; ")
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
    for name in &report.sent {
        log::info!("flows", "flow synced", machine = machine, name = name);
    }
    for name in &report.kept {
        log::info!("flows", "flow kept newer there", machine = machine, name = name);
    }
    for failed in &report.failed {
        log::warning!("flows", "flow not synced", machine = machine, name = failed.name, why = failed.error);
    }
    if let Some(error) = &report.error {
        log::warning!("flows", "flows not synced", machine = machine, why = error);
    }
}
