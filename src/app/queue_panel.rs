use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::Color;
use serde_json::Value;

use super::App;
use super::overlay::Overlay;
use super::toast::{Toast, Undo};
use crate::agents::launch;
use crate::control;
use crate::host::notify::Notification;
use crate::issues::browser::{Browser, Queued, QueuedKind};
use crate::project::Hierarchy;
use crate::queue::{self, Queue, State, Task};
use crate::ui;

const CLIENTS: u64 = 1 << 63;

#[derive(Default)]
pub(super) struct QueueRuntime {
    queue: Queue,
    running: HashMap<u64, u64>,
    next_client: u64,
    zone: String,
    area: Option<Rect>,
    path: Option<std::path::PathBuf>,
    back: Option<Box<Browser>>,
    pub(super) cleanups: super::routine_cleanup::Cleanups,
    removals: VecDeque<(u64, Vec<queue::Entry>)>,
    next_undo: u64,
}

impl QueueRuntime {
    pub(super) fn queue(&self) -> &Queue {
        &self.queue
    }

    pub(crate) fn listed(&self) -> Vec<queue::Listed> {
        self.queue.entries.iter().map(queue::Listed::from).collect()
    }

    pub(super) fn zone(&self) -> &str {
        &self.zone
    }

    pub(super) fn area(&self) -> Option<Rect> {
        self.area
    }

    pub(super) fn wake(&self, clock: i64) -> Option<Duration> {
        self.area.and_then(|_| self.queue.next(clock))
    }

    pub(super) fn starting(&self, id: u64) -> bool {
        self.running.values().any(|running| *running == id)
    }

    pub(super) fn trust(&self, client: u64) -> bool {
        self.running.contains_key(&client)
    }

    pub(super) fn fresh(&self, client: u64) -> bool {
        self.running.get(&client).and_then(|id| self.queue.entry(*id)).is_some_and(queue::Entry::is_run)
    }

    pub(super) fn schedule(&mut self, edit: Option<u64>, task: Task) -> Result<u64, String> {
        match edit {
            Some(id) => self.queue.edit(id, task).map(|()| id),
            None => self.queue.add(task),
        }
    }

    pub(super) fn remove(&mut self, id: u64) -> Result<Vec<queue::Entry>, String> {
        if self.starting(id) {
            return Err("this entry is already starting".into());
        }
        self.queue.remove(id)
    }

    pub(super) fn pause(&mut self, id: u64, paused: bool, now: i64) -> Result<(), String> {
        self.queue.pause(id, paused, now)
    }

    pub(super) fn restore(&mut self, entries: Vec<queue::Entry>) {
        self.queue.restore(entries);
    }

    pub(super) fn clear_started(&mut self) -> Vec<queue::Entry> {
        self.queue.clear_started()
    }

    pub(super) fn keep_removed(&mut self, entries: Vec<queue::Entry>) -> Option<u64> {
        if entries.is_empty() {
            return None;
        }
        self.next_undo += 1;
        let undo = BuildHasherDefault::<DefaultHasher>::default().hash_one((control::server_token(), self.next_undo))
            & ((1 << 53) - 1);
        if self.removals.len() == queue::UNDOS {
            self.removals.pop_front();
        }
        self.removals.push_back((undo, entries));
        Some(undo)
    }

    pub(super) fn take_removed(&mut self, undo: u64) -> Option<Vec<queue::Entry>> {
        let index = self.removals.iter().position(|(id, _)| *id == undo)?;
        self.removals.remove(index).map(|(_, entries)| entries)
    }

    pub(super) fn run_now(&mut self, routine: u64, at: i64) -> Result<u64, String> {
        self.queue.run_now(routine, at)
    }

    pub(super) fn set_branch(&mut self, id: u64, branch: &str) {
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.task.place.branch = Some(branch.to_string());
        }
    }

    pub(super) fn set_workspace(&mut self, id: u64, workspace: (u64, std::path::PathBuf, Option<String>)) {
        if let Some(entry) = self.queue.entry_mut(id) {
            let place = &mut entry.task.place;
            place.workspace_id = Some(workspace.0);
            place.workspace = Some(workspace.1);
            place.workspace_name = workspace.2;
            place.server = control::server_token().into();
        }
    }

    pub(super) fn set_state(&mut self, id: u64, state: State) {
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.state = state;
        }
    }

    pub(super) fn start(&mut self, id: u64, now: Instant) -> u64 {
        self.next_client += 1;
        let client = CLIENTS + self.next_client;
        self.running.insert(client, id);
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.state = State::Starting;
            if entry.is_run() && entry.task.cleanup {
                self.cleanups.arm(id, now);
            }
        }
        client
    }

    pub(super) fn started(&mut self, id: u64, pane: u64) {
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.state = State::Started { pane, server: control::server_token().into() };
        }
    }

    pub(super) fn fail(&mut self, id: u64, reason: &str) {
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.state = State::Failed { reason: reason.to_string() };
        }
    }

    pub(super) fn finish(&mut self, client: u64) -> Option<u64> {
        self.running.remove(&client)
    }

    pub(super) fn save(&self) -> Result<(), String> {
        let Some(path) = &self.path else { return Ok(()) };
        crate::persist::save(path, &self.queue.saved()).map_err(|error| error.to_string())
    }

    pub(super) fn keep_browser(&mut self, back: Option<Box<Browser>>) {
        self.back = back;
    }

    pub(super) fn take_browser(&mut self) -> Option<Box<Browser>> {
        self.back.take()
    }
}

#[derive(Debug)]
pub(crate) struct QueueEvent {
    pub(super) client: Option<u64>,
    pub(super) edit: Option<u64>,
    pub(super) result: Result<Task, String>,
}

impl App {
    pub(crate) fn set_queue(&mut self, queue: Queue) {
        self.panels.queued.queue = queue;
    }

    pub(crate) fn set_queue_path(&mut self, path: std::path::PathBuf) {
        self.panels.queued.path = Some(path);
    }

    pub(crate) fn queue_saved(&self) -> queue::Saved {
        self.panels.queued.queue.saved()
    }

    pub(crate) fn set_queue_zone(&mut self, zone: String) {
        self.panels.queued.zone = zone;
    }

    pub(crate) fn set_queue_area(&mut self, area: Rect) {
        self.panels.queued.area = Some(area);
    }

    pub(super) fn fresh_run(&self, client: Option<u64>) -> bool {
        client.is_some_and(|client| self.panels.queued.fresh(client))
    }

    pub(super) fn drive_queue(&mut self, clock: i64, now: Instant) {
        if self.panels.queued.area.is_none() {
            return;
        }
        for id in self.panels.queued.queue.fire(clock) {
            self.routine_missed(id);
        }
        for id in self.panels.queued.queue.due(clock) {
            if let Err(reason) = self.run_queued(id, now) {
                self.fail_queued(id, &reason);
            }
        }
    }

    pub(super) fn queued_spec(&self, client: u64) -> Option<launch::Spec> {
        let entry = self.panels.queued.queue.entry(*self.panels.queued.running.get(&client)?)?;
        Some(launch::Spec {
            command: entry.task.command.clone(),
            prompt: Some(entry.task.prompt.clone()),
            submit: entry.task.submit,
            trusted: false,
        })
    }

    pub(in crate::app) fn queue_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        let Some(id) = self.panels.queued.finish(client) else { return false };
        match reply {
            Ok(value) => {
                let pane = value.get("pane").or_else(|| value.pointer("/members/0/pane"));
                self.panels.queued.started(id, pane.and_then(Value::as_u64).unwrap_or_default());
                self.record_run_workspace(id, value);
                let name = self.queued_name(id);
                self.queue_notice(&format!("started {name}"), ui::ToastIcon::Check);
            }
            Err(reason) => self.fail_queued(id, reason),
        }
        true
    }

    pub(super) fn fail_queued(&mut self, id: u64, reason: &str) {
        self.panels.queued.fail(id, reason);
        let text = match self.panels.queued.queue.entry(id).filter(|entry| entry.is_run()) {
            Some(entry) => format!("routine {} failed: {reason}", entry.task.label),
            None => format!("queued agent {id} failed: {reason}"),
        };
        self.queue_notice(&text, ui::ToastIcon::Bug);
    }

    fn queued_name(&self, id: u64) -> String {
        match self.panels.queued.queue.entry(id).filter(|entry| entry.is_run()) {
            Some(entry) => format!("routine {}", entry.task.label),
            None => format!("queued agent {id}"),
        }
    }

    fn record_run_workspace(&mut self, id: u64, value: &Value) {
        let Some(workspace) = value.get("workspace").and_then(Value::as_u64) else { return };
        let Some(found) = self.projects.workspace_by(workspace) else { return };
        let place = (workspace, found.path.clone(), found.name.clone());
        self.panels.queued.set_workspace(id, place);
    }

    fn routine_missed(&mut self, id: u64) {
        let Some(run) = self.panels.queued.queue.entry(id) else { return };
        let Some(routine) = run.task.routine.and_then(|routine| self.panels.queued.queue.entry(routine)) else {
            return;
        };
        let zone = &run.task.zone;
        let text = format!(
            "routine {} missed its run at {}; next at {}",
            run.task.label,
            queue::clock::display(run.task.at, zone),
            queue::clock::display(routine.task.at, zone)
        );
        self.queue_notice(&text, ui::ToastIcon::Bug);
    }

    fn queue_removed(&mut self, message: String, removed: Vec<queue::Entry>) {
        if !removed.is_empty() {
            self.messages.toast =
                Some(Toast { undo: Some(Undo::Queue(removed)), ..Toast::new(message, ui::ToastIcon::Check) });
        }
    }

    fn queue_notice(&mut self, message: &str, icon: ui::ToastIcon) {
        self.messages.toast = Some(Toast::new(message, icon));
        self.outbox.notify(Notification::new(message, &self.config.desktop_notifications));
        self.announce(message);
    }

    pub(super) fn queue_prepared(&mut self, event: QueueEvent) {
        let reply = event.result.and_then(|task| {
            if task.at <= crate::time::now() {
                return Err("the scheduled time passed while preparing this entry".into());
            }
            self.panels.queued.schedule(event.edit, task)
        });
        if let Some(client) = event.client {
            self.queue_reply(client, reply);
        } else {
            self.queue_form_done(reply);
        }
    }

    pub(super) fn queued_rows(&self) -> Vec<Queued> {
        let zone = if self.panels.queued.zone.is_empty() { "UTC" } else { &self.panels.queued.zone };
        self.panels.queued.queue.entries.iter().map(|entry| self.queued_row(entry, zone)).collect()
    }

    fn queued_row(&self, entry: &queue::Entry, zone: &str) -> Queued {
        let place = self.queued_place(&entry.task.place);
        let (when, meta, kind) = if let Some(repeat) = entry.task.repeat {
            let when =
                if entry.task.paused { "paused".to_string() } else { queue::clock::display(entry.task.at, zone) };
            let who = entry.task.flow.as_ref().map_or_else(|| entry.task.agent.clone(), |flow| format!("flow {flow}"));
            (when, format!("↻ every {repeat} · {who} · {place}"), QueuedKind::Routine { paused: entry.task.paused })
        } else {
            let kind = if entry.is_run() { QueuedKind::Run } else { QueuedKind::Entry };
            let who = entry.task.flow.as_ref().map_or_else(|| entry.task.agent.clone(), |flow| format!("flow {flow}"));
            let meta = format!("{} · {who} · {place}", self.queued_status(&entry.state));
            (queue::clock::display(entry.task.at, zone), meta, kind)
        };
        Queued { id: entry.id, when, label: entry.task.label.clone(), meta, state: entry.state.clone(), kind }
    }

    fn queued_status(&self, state: &State) -> String {
        match state {
            State::Pending => "pending".into(),
            State::Starting => "starting…".into(),
            State::Started { pane, server }
                if server == control::server_token() && self.projects.pane_by(*pane).is_some() =>
            {
                format!("started · pane {pane}")
            }
            State::Started { .. } => "started · pane closed".into(),
            State::Failed { reason } => format!("failed · {reason}"),
            State::Missed => "missed".into(),
            State::Removed => "removed".into(),
            State::Kept { reason, .. } => format!("kept · {reason}"),
        }
    }

    fn queued_place(&self, place: &queue::Place) -> String {
        let project = self.projects.iter().find(|project| project.path == place.project).map_or_else(
            || crate::text::folder_name(&place.project, self.home.as_deref()),
            |project| self.project_label(project),
        );
        match (&place.branch, &place.workspace_name, &place.workspace) {
            (Some(branch), ..) => format!("{project} › {branch}"),
            (None, Some(name), _) => format!("{project} › {name}"),
            (None, None, Some(path)) if *path != place.project => {
                format!("{project} › {}", crate::text::folder_name(path, self.home.as_deref()))
            }
            _ => project,
        }
    }

    pub(super) fn sync_queued(&mut self) {
        if matches!(self.overlay, Some(Overlay::Issues(_))) {
            let rows = self.queued_rows();
            if let Some(Overlay::Issues(browser)) = &mut self.overlay {
                browser.queued = rows;
            }
        }
    }

    pub(super) fn open_queued(&mut self, id: u64, area: Rect) {
        let Some(entry) = self.panels.queued.queue.entry(id) else { return };
        match &entry.state {
            State::Started { pane, server } | State::Kept { pane, server, .. } if server == control::server_token() => {
                let pane = *pane;
                self.overlay = None;
                self.focus_queued(pane, area);
            }
            _ if entry.is_run() => {}
            State::Started { .. } | State::Kept { .. } | State::Removed | State::Starting => {}
            _ => self.new_queue_form(Some(id)),
        }
    }

    pub(super) fn run_queued_now(&mut self, id: u64) {
        if let Err(reason) = self.run_queued(id, Instant::now()) {
            self.fail_queued(id, &reason);
        }
        self.sync_queued();
    }

    pub(super) fn remove_queued(&mut self, id: u64) {
        if self.panels.queued.starting(id) {
            self.messages.toast = Some(Toast::new("this entry is already starting", ui::ToastIcon::Bug));
        } else {
            match self.panels.queued.remove(id) {
                Ok(entries) if entries.len() > 1 => {
                    let message = format!("removed routine {id} and its {} runs", entries.len() - 1);
                    self.queue_removed(message, entries);
                }
                Ok(entries) => self.queue_removed(format!("removed queue entry {id}"), entries),
                Err(reason) => self.messages.toast = Some(Toast::new(reason, ui::ToastIcon::Bug)),
            }
        }
        self.sync_queued();
    }

    pub(super) fn pause_queued(&mut self, id: u64, paused: bool) {
        let text = match self.panels.queued.pause(id, paused, crate::time::now()) {
            Ok(()) if paused => format!("paused routine {id}"),
            Ok(()) => {
                let next = self
                    .panels
                    .queued
                    .queue
                    .entry(id)
                    .map_or_else(String::new, |entry| queue::clock::display(entry.task.at, &entry.task.zone));
                format!("resumed routine {id}, next at {next}")
            }
            Err(reason) => reason,
        };
        self.messages.toast = Some(Toast::new(text, ui::ToastIcon::Check));
        self.sync_queued();
    }

    pub(super) fn clear_queued(&mut self) {
        let cleared = self.panels.queued.clear_started();
        self.queue_removed(format!("cleared {} started", cleared.len()), cleared);
        self.sync_queued();
    }
}

fn queued_colour(queued: &Queued) -> Option<Color> {
    match (queued.kind, &queued.state) {
        (QueuedKind::Routine { paused: true }, _) | (_, State::Removed) => Some(Color::DarkGray),
        (_, State::Failed { .. }) => Some(Color::Red),
        (_, State::Missed | State::Kept { .. }) => Some(Color::Yellow),
        (_, State::Started { .. }) => Some(Color::Green),
        (_, State::Pending | State::Starting) => None,
    }
}

pub(super) fn queue_tab(browser: &Browser) -> (ui::IssuesBody, Option<ui::Note>, String) {
    let shown = browser.shown_queue();
    let selected = shown.len().checked_sub(1).map(|last| browser.search.selected().min(last));
    let items = shown
        .iter()
        .map(|queued| ui::IssueRow {
            key: queued.when.clone(),
            title: queued.label.clone(),
            meta: queued.meta.clone(),
            colour: queued_colour(queued),
        })
        .collect();
    let empty = if browser.queued.is_empty() {
        "nothing queued: new schedules a prompt, and schedule on an issue queues it"
    } else {
        "nothing matches"
    };
    let hint = match selected.map(|i| (shown[i].kind, &shown[i].state)) {
        Some((QueuedKind::Routine { .. }, _)) => "enter edits it · start now runs it once more",
        Some((QueuedKind::Run, State::Started { .. } | State::Kept { .. })) => {
            "enter opens its pane · a run of a routine"
        }
        Some((QueuedKind::Run, _)) => "a run of a routine · every project's scheduled agents",
        Some((_, State::Started { .. })) => "enter opens its pane · every project's scheduled agents",
        Some((_, State::Starting)) => "starting… · every project's scheduled agents",
        Some(_) => "enter edits it · start now runs it at once",
        None => "every project's scheduled agents",
    };
    let body = ui::IssuesBody::List {
        filter: browser.search.query().to_string(),
        items,
        selected,
        scroll: browser.search.scroll(),
        empty: empty.into(),
    };
    (body, None, hint.into())
}
