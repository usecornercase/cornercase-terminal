use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use crate::queue;
use crate::todo::{self};
use crate::ui::{self};

pub(super) const TOAST_FOR: Duration = Duration::from_secs(2);

pub(super) const UNDO_FOR: Duration = Duration::from_secs(6);

pub(super) const BUG_FOR: Duration = Duration::from_secs(6);

pub(super) const COPIED: &str = "copied to clipboard";

pub(super) const NOT_READING: &str = "the program in this pane is not reading what it gets";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Undo {
    Todos(todo::Removed),
    Queue(Vec<queue::Entry>),
    Agents(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Toast {
    pub(super) message: String,
    pub(super) icon: ui::ToastIcon,
    pub(super) at: Instant,
    pub(super) undo: Option<Undo>,
    pub(super) go: Option<String>,
}

impl Toast {
    pub(super) fn new(message: impl Into<String>, icon: ui::ToastIcon) -> Self {
        Self { message: message.into(), icon, at: Instant::now(), undo: None, go: None }
    }

    pub(super) fn lasts(&self) -> Duration {
        match self.icon {
            _ if self.undo.is_some() || self.go.is_some() => UNDO_FOR,
            ui::ToastIcon::Bug => BUG_FOR,
            ui::ToastIcon::Restart => Duration::MAX,
            ui::ToastIcon::Check | ui::ToastIcon::Agent(_) => TOAST_FOR,
        }
    }

    pub(super) fn view(&self) -> ui::Toast<'_> {
        let button = match self.icon {
            _ if matches!(self.undo, Some(Undo::Agents(_))) => Some(ui::ToastButton::Reopen),
            _ if self.undo.is_some() => Some(ui::ToastButton::Undo),
            _ if self.go.is_some() => Some(ui::ToastButton::Go),
            ui::ToastIcon::Restart => Some(ui::ToastButton::Cancel),
            ui::ToastIcon::Check | ui::ToastIcon::Agent(_) | ui::ToastIcon::Bug => None,
        };
        ui::Toast { message: &self.message, icon: self.icon, button }
    }
}

impl App {
    pub(super) fn toast_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> bool {
        let Some(toast) = &self.messages.toast else { return false };
        if ev.kind != MouseEventKind::Down(MouseButton::Left) || !ui::toast_button(area, toast.view()).contains(pos) {
            return false;
        }
        match self.messages.toast.take() {
            Some(Toast { undo: Some(Undo::Todos(removed)), .. }) => self.panels.todos.restore(removed),
            Some(Toast { undo: Some(Undo::Queue(entries)), .. }) => self.panels.queued.restore(entries),
            Some(Toast { undo: Some(Undo::Agents(workspace)), .. }) => self.reopen_from_window(workspace, area),
            Some(Toast { go: Some(token), .. }) => self.notification_clicked(&token),
            Some(Toast { icon: ui::ToastIcon::Restart, .. }) => self.cancel_restarts(),
            _ => {}
        }
        true
    }

    pub(super) fn copy(&mut self, text: &str) {
        self.outbox.copy(text);
        self.messages.toast = Some(Toast::new(COPIED, ui::ToastIcon::Check));
    }
}
