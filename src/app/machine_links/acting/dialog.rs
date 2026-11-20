use serde_json::Value;

use super::{Aim, sighted, told};
use crate::app::App;
use crate::app::closed_agents::forgotten;
use crate::app::dialogs::{close_pane_question, close_question, close_tab_question, confirm_view, rename_form};
use crate::app::menus::{RENAME_TAB, RENAME_TAB_HINT, RENAME_WORKSPACE, RENAME_WORKSPACE_HINT};
use crate::app::overlay::{CLOSE_SUBMIT, Overlay, RENAME_SUBMIT};
use crate::app::worktrees::{Check, Removing, removal_submit, removal_view};
use crate::control::arrange::{Locked, Removal};
use crate::control::contract::{Code, Refusal};
use crate::control::{TabInfo, WorkspaceInfo};
use crate::editor::Editor;
use crate::git::worktree;
use crate::machines::act::{self, Act, Scope};
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
    RenameTab { tab: u64, input: Editor },
    CloseTab { tab: u64 },
    ClosePane { tab: u64, pane: u64 },
}

impl Kind {
    pub(in crate::app) fn scope(&self) -> Scope {
        match self {
            Self::Close | Self::Rename(_) | Self::Remove { .. } => Scope::Workspace,
            Self::RenameTab { tab, .. } | Self::CloseTab { tab } => Scope::Tab(*tab),
            Self::ClosePane { pane, .. } => Scope::Pane(*pane),
        }
    }
}

impl Dialog {
    pub(in crate::app::machine_links) fn open(aim: Aim, kind: Kind) -> Overlay {
        Overlay::MachineWorkspace(Box::new(Self { aim, kind }))
    }

    pub(in crate::app) fn name(&self) -> &'static str {
        match self.kind {
            Kind::Close => "close machine workspace",
            Kind::Rename(_) => "rename machine workspace",
            Kind::Remove { .. } => "remove machine workspace",
            Kind::RenameTab { .. } => "rename machine tab",
            Kind::CloseTab { .. } => "close machine tab",
            Kind::ClosePane { .. } => "close machine pane",
        }
    }

    pub(in crate::app) fn submit_label(&self) -> &'static str {
        match &self.kind {
            Kind::Close | Kind::CloseTab { .. } | Kind::ClosePane { .. } => CLOSE_SUBMIT,
            Kind::Rename(_) | Kind::RenameTab { .. } => RENAME_SUBMIT,
            Kind::Remove { check, removal } => removal_submit(*check, locked(removal.as_ref())),
        }
    }

    pub(in crate::app) fn input(&mut self) -> Option<&mut Editor> {
        match &mut self.kind {
            Kind::Rename(input) | Kind::RenameTab { input, .. } => Some(input),
            Kind::Close | Kind::Remove { .. } | Kind::CloseTab { .. } | Kind::ClosePane { .. } => None,
        }
    }

    pub(in crate::app) fn takes_input(&self) -> bool {
        matches!(self.kind, Kind::Rename(_) | Kind::RenameTab { .. })
    }
}

impl App {
    pub(in crate::app) fn remote_close(&mut self, aim: Aim) {
        if let Err(why) = self.aimed(aim, Scope::Workspace) {
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
        if let Err(why) = self.aimed(aim, Scope::Workspace) {
            return self.not_sent(&why);
        }
        let Some(workspace) = self.remote_info(aim) else { return };
        let name = workspace.custom_name.as_ref().unwrap_or(&workspace.name);
        self.overlay = Some(Dialog::open(aim, Kind::Rename(Editor::new(name))));
    }

    pub(in crate::app::machine_links) fn remote_info(&self, aim: Aim) -> Option<&WorkspaceInfo> {
        sighted(self.held_by(aim.link)?, aim).ok().map(|(_, workspace)| workspace)
    }

    pub(in crate::app::machine_links) fn remote_tab_info(&self, aim: Aim, tab: u64) -> Option<&TabInfo> {
        self.remote_info(aim)?.tabs.iter().find(|there| there.id == tab)
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
                let removing = Removing {
                    label: &workspace.name,
                    path: &path,
                    roles: removal.as_ref().map_or(0, |removal| removal.roles.len()),
                    teardown: removal.as_ref().is_some_and(|removal| removal.teardown),
                    closed: &closed,
                    on,
                };
                removal_view(&removing, *check, removal.as_ref().and_then(|r| r.lock.as_ref()).map(lock_of).as_ref())
            }
            kind => tab_view(workspace, kind, on)?,
        })
    }

    pub(in crate::app) fn submit_machine_workspace(&mut self, dialog: Dialog) -> Option<Overlay> {
        let Dialog { aim, kind } = dialog;
        let act = match kind {
            Kind::Close => Act::Close,
            Kind::Rename(input) => Act::Rename(input.text().trim().to_string()),
            Kind::RenameTab { tab, input } => Act::RenameTab { tab, name: input.text().trim().to_string() },
            Kind::CloseTab { tab } => Act::CloseTab(tab),
            Kind::ClosePane { pane, .. } => Act::ClosePane(pane),
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

fn tab_view(workspace: &WorkspaceInfo, kind: &Kind, on: Option<&str>) -> Option<ui::Overlay> {
    let tab_of = |id: u64| workspace.tabs.iter().find(|tab| tab.id == id);
    Some(match kind {
        Kind::RenameTab { input, .. } => rename_form(RENAME_TAB, RENAME_TAB_HINT, input),
        Kind::CloseTab { tab } => {
            confirm_view("Close tab", close_tab_question(&tab_of(*tab)?.name, on), CLOSE_SUBMIT, None)
        }
        Kind::ClosePane { tab, pane } => {
            let pane = tab_of(*tab)?.panes.iter().find(|there| there.id == *pane)?;
            let program = pane.program.as_deref().unwrap_or("?");
            confirm_view("Close pane", close_pane_question(program, on), CLOSE_SUBMIT, None)
        }
        Kind::Close | Kind::Rename(_) | Kind::Remove { .. } => return None,
    })
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
