use std::path::PathBuf;
use std::time::Instant;

use super::{ChangesEvent, Chosen};
use crate::app::control::Replied;
use crate::app::overlay::{DISCARD_SUBMIT, Overlay};
use crate::app::toast::Toast;
use crate::app::{App, AppEvent};
use crate::changes::act::{FileAction, Outcome};
use crate::changes::diff::Staged;
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
        let seen = Seen { workspace: target.workspace, path: target.path, digest: target.digest };
        self.act_on_file(seen, FileAction::Discard);
        None
    }

    pub(super) fn stage_clicked(&mut self, chosen: Option<Chosen>) {
        let Some(chosen) = chosen else { return };
        let action = if chosen.file.staged == Some(Staged::Yes) { FileAction::Unstage } else { FileAction::Stage };
        let seen = Seen {
            workspace: chosen.target.workspace,
            path: chosen.file.path.clone(),
            digest: chosen.file.viewed_digest,
        };
        self.act_on_file(seen, action);
    }

    fn act_on_file(&mut self, seen: Seen, action: FileAction) {
        match self.git_workspace(seen.workspace).map(|ws| ws.path.clone()) {
            Ok(dir) => self.spawn_file_action(None, dir, seen, action),
            Err(refusal) => self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug)),
        }
    }

    pub(in crate::app) fn spawn_file_action(&self, request: Option<u64>, dir: PathBuf, seen: Seen, action: FileAction) {
        let Seen { workspace, path, digest } = seen;
        let job = Job::new(Level::Info, "changes", action.name()).with("workspace", workspace).with("path", &path);
        let target = path.clone();
        self.spawn_job(
            job,
            move || action.run(&dir, &target, digest),
            move |result, _| Some(AppEvent::Changes(ChangesEvent::Acted { request, workspace, path, action, result })),
        );
    }

    pub(super) fn file_action_answered(
        &mut self,
        request: Option<u64>,
        (workspace, path): (u64, &str),
        action: FileAction,
        result: Result<Outcome>,
    ) {
        if matches!(result, Ok(Outcome::Done(_))) {
            if action == FileAction::Discard {
                self.panels.changes.discarded(workspace, path);
            } else {
                self.panels.changes.rescan(workspace);
            }
            self.refresh_changes(Instant::now());
        }
        match request {
            Some(key) => self.changes_action_replied(Replied { key, workspace, path }, action, result),
            None => {
                if let Some(toast) = action_toast(path, action, &result) {
                    self.messages.toast = Some(toast);
                }
            }
        }
    }
}

fn action_toast(path: &str, action: FileAction, result: &Result<Outcome>) -> Option<Toast> {
    let (message, icon) = match result {
        Ok(Outcome::Done(file)) if action == FileAction::Discard => {
            (discard::done(&file.path, file.old_path.as_deref(), file.status), ui::ToastIcon::Check)
        }
        Ok(Outcome::Done(_)) => return None,
        Ok(Outcome::Changed) => {
            (format!("{path} changed meanwhile: nothing was {}", action.past()), ui::ToastIcon::Bug)
        }
        Ok(Outcome::Gone) => (format!("{path} has no changes any more"), ui::ToastIcon::Check),
        Err(e) => (format!("cannot {} {path}: {e}", action.name()), ui::ToastIcon::Bug),
    };
    Some(Toast::new(message, icon))
}
