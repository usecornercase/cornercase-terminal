use serde_json::Value;

use super::{Aim, sighted, told};
use crate::app::App;
use crate::app::closed_agents::forgotten;
use crate::app::dialogs::{close_question, confirm_view, rename_form};
use crate::app::menus::{RENAME_WORKSPACE, RENAME_WORKSPACE_HINT};
use crate::app::overlay::{CLOSE_SUBMIT, Overlay, RENAME_SUBMIT};
use crate::app::worktrees::{Check, Removing, removal_submit, removal_view};
use crate::control::WorkspaceInfo;
use crate::control::arrange::{Locked, Removal};
use crate::control::contract::{Code, Refusal};
use crate::editor::Editor;
use crate::git::worktree;
use crate::machines::act::{self, Act};
use crate::ui;

#[derive(Debug)]
pub(in crate::app) struct Dialog {
    pub(in crate::app) aim: Aim,
    pub(in crate::app) kind: Kind,
}

#[derive(Debug)]
pub(in crate::app) enum Kind {
    Close,
    Rename(Editor),
    Remove { check: Check, removal: Option<Removal> },
}

impl Dialog {
    fn open(aim: Aim, kind: Kind) -> Overlay {
        Overlay::MachineWorkspace(Box::new(Self { aim, kind }))
    }

    pub(in crate::app) fn name(&self) -> &'static str {
        match self.kind {
            Kind::Close => "close machine workspace",
            Kind::Rename(_) => "rename machine workspace",
            Kind::Remove { .. } => "remove machine workspace",
        }
    }

    pub(in crate::app) fn submit_label(&self) -> &'static str {
        match &self.kind {
            Kind::Close => CLOSE_SUBMIT,
            Kind::Rename(_) => RENAME_SUBMIT,
            Kind::Remove { check, removal } => removal_submit(*check, locked(removal.as_ref())),
        }
    }

    pub(in crate::app) fn input(&mut self) -> Option<&mut Editor> {
        match &mut self.kind {
            Kind::Rename(input) => Some(input),
            Kind::Close | Kind::Remove { .. } => None,
        }
    }

    pub(in crate::app) fn takes_input(&self) -> bool {
        matches!(self.kind, Kind::Rename(_))
    }
}

impl App {
    pub(in crate::app) fn remote_close(&mut self, aim: Aim) {
        if let Err(why) = self.aimed(aim) {
            return self.not_sent(&why);
        }
        if self.remote_info(aim).is_some_and(|workspace| workspace.worktree) {
            self.overlay = Some(Dialog::open(aim, Kind::Remove { check: Check::Running, removal: None }));
            self.send_act(aim, Act::Check);
        } else {
            self.overlay = Some(Dialog::open(aim, Kind::Close));
        }
    }

    pub(in crate::app) fn remote_rename(&mut self, aim: Aim) {
        if let Err(why) = self.aimed(aim) {
            return self.not_sent(&why);
        }
        let Some(workspace) = self.remote_info(aim) else { return };
        let name = workspace.custom_name.as_ref().unwrap_or(&workspace.name);
        self.overlay = Some(Dialog::open(aim, Kind::Rename(Editor::new(name))));
    }

    fn remote_info(&self, aim: Aim) -> Option<&WorkspaceInfo> {
        sighted(self.held_by(aim.link)?, aim).ok().map(|(_, workspace)| workspace)
    }

    pub(in crate::app) fn machine_workspace_view(&self, dialog: &Dialog) -> Option<ui::Overlay> {
        let held = self.held_by(dialog.aim.link)?;
        let (seen, workspace) = sighted(held, dialog.aim).ok()?;
        let closed = forgotten(workspace.closed_agents.unwrap_or(0));
        let on = Some(held.name.as_str());
        Some(match &dialog.kind {
            Kind::Close => {
                let question = close_question(&workspace.name, workspace.tabs.len(), &closed, on);
                confirm_view("Close workspace", question, CLOSE_SUBMIT, None)
            }
            Kind::Rename(input) => rename_form(RENAME_WORKSPACE, RENAME_WORKSPACE_HINT, input),
            Kind::Remove { check, removal } => {
                let path = crate::text::display_path(&workspace.path, seen.home.as_deref());
                let lock = removal.as_ref().and_then(|removal| removal.lock.as_ref()).map(lock_of);
                let removing = Removing {
                    label: &workspace.name,
                    path: &path,
                    roles: removal.as_ref().map_or(0, |removal| removal.roles.len()),
                    teardown: removal.as_ref().is_some_and(|removal| removal.teardown),
                    closed: &closed,
                    on,
                };
                removal_view(&removing, *check, lock.as_ref())
            }
        })
    }

    pub(in crate::app) fn submit_machine_workspace(&mut self, dialog: Dialog) -> Option<Overlay> {
        let Dialog { aim, kind } = dialog;
        let act = match kind {
            Kind::Close => Act::Close,
            Kind::Rename(input) => Act::Rename(input.text().trim().to_string()),
            Kind::Remove { check: Check::Running | Check::Confirmed, removal } => {
                return Some(Dialog::open(aim, Kind::Remove { check: Check::Confirmed, removal }));
            }
            Kind::Remove { check, removal } => {
                Act::Remove { force: check == Check::Changed, unlock: locked(removal.as_ref()) }
            }
        };
        self.send_act(aim, act);
        None
    }

    pub(super) fn remote_checked(&mut self, aim: Aim, result: Result<Value, Refusal>, label: &str, machine: &str) {
        if !self.remote_checking(aim) {
            return;
        }
        match result.and_then(removal_of) {
            Ok(removal) => self.remote_found(aim, removal),
            Err(refusal) => {
                self.overlay = None;
                self.messages.toast = told(&Act::Check, label, machine, &Err(refusal));
            }
        }
    }

    fn remote_checking(&self, aim: Aim) -> bool {
        let running = |kind: &Kind| matches!(kind, Kind::Remove { check: Check::Running | Check::Confirmed, .. });
        matches!(&self.overlay, Some(Overlay::MachineWorkspace(dialog)) if dialog.aim == aim && running(&dialog.kind))
    }

    fn remote_found(&mut self, aim: Aim, found: Removal) {
        let Some(Overlay::MachineWorkspace(dialog)) = &mut self.overlay else { return };
        let Kind::Remove { check, removal } = &mut dialog.kind else { return };
        let go_on = *check == Check::Confirmed && !found.changed && found.lock.is_none();
        *check = if found.changed { Check::Changed } else { Check::Clean };
        *removal = Some(found);
        if go_on {
            self.overlay = None;
            self.send_act(aim, Act::Remove { force: false, unlock: false });
        }
    }
}

fn removal_of(value: Value) -> Result<Removal, Refusal> {
    serde_json::from_value(value).map_err(|_| Refusal::new(Code::Malformed, act::UNREADABLE))
}

fn locked(removal: Option<&Removal>) -> bool {
    removal.is_some_and(|removal| removal.lock.is_some())
}

fn lock_of(locked: &Locked) -> worktree::Lock {
    let holder = locked
        .pid
        .zip(locked.running)
        .map(|(pid, running)| if running { worktree::Holder::Running(pid) } else { worktree::Holder::Gone(pid) });
    let role = locked.role.clone().map(|role| (role, locked.path.clone()));
    worktree::Lock { reason: locked.reason.clone(), holder, role }
}
