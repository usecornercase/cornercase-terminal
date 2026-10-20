use std::path::PathBuf;
use std::time::Instant;

use super::{ChangesEvent, Chosen};
use crate::app::control::Replied;
use crate::app::overlay::{DISCARD_SUBMIT, Overlay};
use crate::app::toast::Toast;
use crate::app::{App, AppEvent};
use crate::changes::discard;
use crate::error::Result;
use crate::log::{Job, Level};
use crate::ui;

pub(in crate::app) struct Seen {
    pub(in crate::app) workspace: u64,
    pub(in crate::app) path: String,
    pub(in crate::app) digest: u64,
}

impl App {
    pub(super) fn ask_discard(&mut self, chosen: Option<Chosen>) {
        if let Some(chosen) = chosen {
            let target = discard::Target::of(chosen.target.workspace, &chosen.file);
            self.overlay = Some(Overlay::DiscardChanges(target));
        }
    }

    pub(in crate::app) fn discard_view(target: &discard::Target) -> ui::Overlay {
        ui::Overlay::Confirm(ui::Confirm {
            title: "Discard changes",
            message: target.question(),
            note: None,
            submit: DISCARD_SUBMIT,
            extra: None,
            dangerous: true,
        })
    }

    pub(in crate::app) fn submit_discard(&mut self, target: discard::Target) -> Option<Overlay> {
        match self.git_workspace(target.workspace).map(|ws| ws.path.clone()) {
            Ok(dir) => {
                let seen = Seen { workspace: target.workspace, path: target.path, digest: target.digest };
                self.spawn_discard(None, dir, seen);
            }
            Err(refusal) => self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug)),
        }
        None
    }

    pub(in crate::app) fn spawn_discard(&self, request: Option<u64>, dir: PathBuf, seen: Seen) {
        let Seen { workspace, path, digest } = seen;
        let job = Job::new(Level::Info, "changes", "discard").with("workspace", workspace).with("path", &path);
        let target = path.clone();
        self.spawn_job(
            job,
            move || discard::discard(&dir, &target, digest),
            move |result, _| Some(AppEvent::Changes(ChangesEvent::Discarded { request, workspace, path, result })),
        );
    }

    pub(super) fn discard_answered(
        &mut self,
        request: Option<u64>,
        workspace: u64,
        path: &str,
        result: Result<discard::Outcome>,
    ) {
        if matches!(result, Ok(discard::Outcome::Discarded(_))) {
            self.panels.changes.discarded(workspace, path);
            self.refresh_changes(Instant::now());
        }
        match request {
            Some(key) => self.changes_discard_replied(Replied { key, workspace, path }, result),
            None => self.messages.toast = Some(discard_toast(path, &result)),
        }
    }
}

fn discard_toast(path: &str, result: &Result<discard::Outcome>) -> Toast {
    let (message, icon) = match result {
        Ok(discard::Outcome::Discarded(file)) => {
            (discard::done(&file.path, file.old_path.as_deref(), file.status), ui::ToastIcon::Check)
        }
        Ok(discard::Outcome::Changed) => {
            (format!("{path} changed meanwhile: nothing was discarded"), ui::ToastIcon::Bug)
        }
        Ok(discard::Outcome::Gone) => (format!("{path} has no changes any more"), ui::ToastIcon::Check),
        Err(e) => (format!("cannot discard {path}: {e}"), ui::ToastIcon::Bug),
    };
    Toast::new(message, icon)
}
