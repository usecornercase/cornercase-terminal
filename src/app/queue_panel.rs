use std::collections::HashMap;
use std::time::Instant;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::App;
use super::layout::Side;
use super::toast::Toast;
use crate::agents::launch;
use crate::control;
use crate::host::notify::Notification;
use crate::project::Hierarchy;
use crate::queue::{self, Queue, State, Task};
use crate::ui;

pub(super) const CLIENTS: u64 = 1 << 63;

#[derive(Default)]
pub(super) struct QueueRuntime {
    pub(super) queue: Queue,
    pub(super) scroll: usize,
    pub(super) running: HashMap<u64, u64>,
    pub(super) next_client: u64,
    pub(super) zone: String,
    pub(super) area: Option<Rect>,
    pub(super) path: Option<std::path::PathBuf>,
}

#[derive(Debug)]
pub(crate) struct QueueEvent {
    pub(super) client: Option<u64>,
    pub(super) edit: Option<u64>,
    pub(super) result: Result<Task, String>,
}

impl App {
    pub(crate) fn set_queue(&mut self, queue: Queue) {
        self.queued.queue = queue;
    }

    pub(crate) fn set_queue_path(&mut self, path: std::path::PathBuf) {
        self.queued.path = Some(path);
    }

    pub(crate) fn queue_saved(&self) -> queue::Saved {
        self.queued.queue.saved()
    }

    pub(crate) fn set_queue_zone(&mut self, zone: String) {
        self.queued.zone = zone;
    }

    pub(super) fn toggle_queue(&mut self) {
        self.nav = None;
        self.show_side((self.side != Some(Side::Queue)).then_some(Side::Queue));
    }

    pub(super) fn drive_queue(&mut self, clock: i64, now: Instant) {
        if self.queued.area.is_none() {
            return;
        }
        for id in self.queued.queue.due(clock) {
            if let Err(reason) = self.run_queued(id, now) {
                self.fail_queued(id, &reason);
            }
        }
    }

    pub(super) fn queued_spec(&self, client: u64) -> Option<launch::Spec> {
        let entry = self.queued.queue.entry(*self.queued.running.get(&client)?)?;
        Some(launch::Spec {
            command: entry.task.command.clone(),
            prompt: Some(entry.task.prompt.clone()),
            submit: entry.task.submit,
        })
    }

    pub(in crate::app) fn queue_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        let Some(id) = self.queued.running.remove(&client) else { return false };
        match reply {
            Ok(value) => {
                let pane = value.get("pane").and_then(Value::as_u64).unwrap_or_default();
                if let Some(entry) = self.queued.queue.entry_mut(id) {
                    entry.state = State::Started { pane, server: control::server_token().into() };
                }
                self.queue_notice(&format!("started queued agent {id}"), ui::ToastIcon::Check);
            }
            Err(reason) => self.fail_queued(id, reason),
        }
        true
    }

    pub(super) fn fail_queued(&mut self, id: u64, reason: &str) {
        if let Some(entry) = self.queued.queue.entry_mut(id) {
            entry.state = State::Failed { reason: reason.to_string() };
        }
        self.queue_notice(&format!("queued agent {id} failed: {reason}"), ui::ToastIcon::Bug);
    }

    fn queue_notice(&mut self, message: &str, icon: ui::ToastIcon) {
        self.toast = Some(Toast::new(message, icon));
        self.outbox.notifications.extend(Notification::new(message, &self.config.desktop_notifications));
    }

    pub(super) fn queue_prepared(&mut self, event: QueueEvent) {
        let reply = event.result.and_then(|task| {
            if task.at <= crate::time::now() {
                return Err("the scheduled time passed while preparing this entry".into());
            }
            match event.edit {
                Some(id) => self.queued.queue.edit(id, task).map(|()| id),
                None => self.queued.queue.add(task),
            }
        });
        if let Some(client) = event.client {
            self.queue_reply(client, reply);
        } else {
            self.queue_form_done(reply);
        }
    }

    pub(super) fn queue_view(&self) -> ui::queue::View {
        let zone = if self.queued.zone.is_empty() { "UTC" } else { &self.queued.zone };
        let entries = self
            .queued
            .queue
            .entries
            .iter()
            .map(|entry| {
                let status = match &entry.state {
                    State::Pending => "pending".into(),
                    State::Starting => "starting…".into(),
                    State::Started { pane, server } => {
                        if server == control::server_token() && self.projects.pane_by(*pane).is_some() {
                            format!("started · pane {pane}")
                        } else {
                            "started · pane closed".into()
                        }
                    }
                    State::Failed { reason } => format!("failed · {reason}"),
                    State::Missed => "missed".into(),
                };
                ui::queue::Row {
                    id: entry.id,
                    when: queue::clock::display(entry.task.at, zone),
                    label: entry.task.label.clone(),
                    meta: format!(
                        "{} · {}",
                        entry.task.agent,
                        if entry.task.place.branch.is_some() { "worktree" } else { "tab" }
                    ),
                    status,
                    state: entry.state.clone(),
                }
            })
            .collect();
        ui::queue::View { entries, scroll: self.queued.scroll, pending: self.queued.queue.pending() }
    }

    pub(super) fn queue_mouse(&mut self, event: MouseEvent, pos: Position, panel: Rect, area: Rect) {
        let view = self.queue_view();
        let rows = ui::queue::rows(panel, &view);
        if let Some(delta) = super::input::wheel(event.kind) {
            self.queued.scroll = rows.scrolled(delta);
            return;
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match ui::queue::hit(panel, &view, pos) {
            Some(ui::queue::Hit::Close) => self.close_panel(),
            Some(ui::queue::Hit::New) => self.new_queue_form(None),
            Some(ui::queue::Hit::Clear) => self.queued.queue.clear_started(),
            Some(ui::queue::Hit::Remove(id)) => {
                if self.queued.running.values().any(|running| *running == id) {
                    self.queue_notice("this entry is already starting", ui::ToastIcon::Bug);
                } else {
                    let _ = self.queued.queue.remove(id);
                }
            }
            Some(ui::queue::Hit::Run(id)) => {
                if let Err(reason) = self.run_queued(id, Instant::now()) {
                    self.fail_queued(id, &reason);
                }
            }
            Some(ui::queue::Hit::Entry(id)) => {
                if let Some(entry) = self.queued.queue.entry(id) {
                    if let State::Started { pane, server } = &entry.state {
                        if server == control::server_token() {
                            self.focus_queued(*pane, area);
                        }
                    } else {
                        self.new_queue_form(Some(id));
                    }
                }
            }
            None => {}
        }
    }
}
