use std::collections::HashMap;
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
use crate::issues::browser::{Browser, Queued};
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
}

impl QueueRuntime {
    pub(super) fn queue(&self) -> &Queue {
        &self.queue
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

    pub(super) fn schedule(&mut self, edit: Option<u64>, task: Task) -> Result<u64, String> {
        match edit {
            Some(id) => self.queue.edit(id, task).map(|()| id),
            None => self.queue.add(task),
        }
    }

    pub(super) fn remove(&mut self, id: u64) -> Result<queue::Entry, String> {
        if self.starting(id) {
            return Err("this entry is already starting".into());
        }
        self.queue.remove(id)
    }

    pub(super) fn restore(&mut self, entries: Vec<queue::Entry>) {
        self.queue.restore(entries);
    }

    pub(super) fn start(&mut self, id: u64) -> u64 {
        self.next_client += 1;
        let client = CLIENTS + self.next_client;
        self.running.insert(client, id);
        if let Some(entry) = self.queue.entry_mut(id) {
            entry.state = State::Starting;
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

    pub(super) fn drive_queue(&mut self, clock: i64, now: Instant) {
        if self.panels.queued.area.is_none() {
            return;
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
                let pane = value.get("pane").and_then(Value::as_u64).unwrap_or_default();
                self.panels.queued.started(id, pane);
                self.queue_notice(&format!("started queued agent {id}"), ui::ToastIcon::Check);
            }
            Err(reason) => self.fail_queued(id, reason),
        }
        true
    }

    pub(super) fn fail_queued(&mut self, id: u64, reason: &str) {
        self.panels.queued.fail(id, reason);
        self.queue_notice(&format!("queued agent {id} failed: {reason}"), ui::ToastIcon::Bug);
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
        Queued {
            id: entry.id,
            when: queue::clock::display(entry.task.at, zone),
            label: entry.task.label.clone(),
            meta: format!("{} · {} · {place}", self.queued_status(&entry.state), entry.task.agent),
            state: entry.state.clone(),
        }
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
        match self.panels.queued.queue.entry(id).map(|entry| &entry.state) {
            Some(State::Started { pane, server }) if server == control::server_token() => {
                let pane = *pane;
                self.overlay = None;
                self.focus_queued(pane, area);
            }
            Some(State::Started { .. } | State::Starting) | None => {}
            Some(_) => self.new_queue_form(Some(id)),
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
        } else if let Ok(entry) = self.panels.queued.remove(id) {
            self.queue_removed(format!("removed queue entry {id}"), vec![entry]);
        }
        self.sync_queued();
    }

    pub(super) fn clear_queued(&mut self) {
        let cleared = self.panels.queued.queue.clear_started();
        self.queue_removed(format!("cleared {} started", cleared.len()), cleared);
        self.sync_queued();
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
            colour: match queued.state {
                State::Failed { .. } => Some(Color::Red),
                State::Missed => Some(Color::Yellow),
                State::Started { .. } => Some(Color::Green),
                State::Pending | State::Starting => None,
            },
        })
        .collect();
    let empty = if browser.queued.is_empty() {
        "nothing queued: new schedules a prompt, and schedule on an issue queues it"
    } else {
        "nothing matches"
    };
    let hint = match selected.map(|i| &shown[i].state) {
        Some(State::Started { .. }) => "enter opens its pane · every project's scheduled agents",
        Some(State::Starting) => "starting… · every project's scheduled agents",
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
