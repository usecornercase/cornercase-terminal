use std::time::SystemTime;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::MenuAction;
use super::menus::Target;
use super::settings_dialog::settings_view;
use super::toast::Toast;
use super::update_dialog::UpdateStep;
use super::worktrees::Check;
use crate::changes::BranchPicker;
use crate::clipboard;
use crate::error::Result;
use crate::issues::browser::Browser;
use crate::list::Search;
use crate::picker::Picker;
use crate::settings::Settings;
use crate::shortcuts::Group as KeysGroup;
use crate::time;
use crate::ui::{self, FormHit};
use crate::usage;
use crate::worktree;

pub(super) const CREATE_SUBMIT: &str = "create";

pub(super) const RENAME_SUBMIT: &str = "rename";

pub(super) const REMOVE_SUBMIT: &str = "remove";

pub(super) const DELETE_SUBMIT: &str = "delete";

pub(super) const CLOSE_SUBMIT: &str = "close";

pub(super) const FORCE_REMOVE_SUBMIT: &str = "remove anyway";

pub(super) const UNLOCK_SUBMIT: &str = "unlock and remove";

const NEW_GROUP_HINT: &str = "right-click a project or its ⋯ to move it into the group";

const WORKTREE_TOGGLE: &str = "with its own worktree";

pub(super) const UPDATE_SUBMIT: &str = "update";

pub(super) const RETRY_UPDATE_SUBMIT: &str = "try again";

pub(super) const RESTART_SUBMIT: &str = "restart now";

pub(super) const LATER: &str = "later";

const COPY_COMMAND_SUBMIT: &str = "copy command";

#[derive(Debug)]
pub(super) enum Overlay {
    Menu { at: Position, actions: Vec<MenuAction> },
    NewGroup { input: String },
    GroupStyle { group: u64 },
    DeleteGroup { group: u64 },
    CloseProject { project: u64 },
    CloseWorkspace { project: u64, workspace: u64 },
    CloseTab { project: u64, workspace: u64, tab: u64 },
    ClosePane { pane: u64 },
    NewWorkspace { project: u64, input: String, worktree: Option<bool>, error: Option<String>, creating: bool },
    Settings(Box<Settings>),
    Rename { target: Target, input: String },
    RemoveWorkspace { project: u64, workspace: u64, check: Check, lock: Option<worktree::Lock> },
    Picker { picker: Picker, group: Option<u64> },
    Issues(Box<Browser>),
    Search(Search),
    Update(UpdateStep),
    Restart,
    Usage,
    Remote,
    Branches(BranchPicker),
    Keys(Option<KeysGroup>),
}

impl Overlay {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Menu { .. } => "menu",
            Self::NewGroup { .. } => "new group",
            Self::GroupStyle { .. } => "group style",
            Self::DeleteGroup { .. } => "delete group",
            Self::CloseProject { .. } => "close project",
            Self::CloseWorkspace { .. } => "close workspace",
            Self::CloseTab { .. } => "close tab",
            Self::ClosePane { .. } => "close pane",
            Self::NewWorkspace { .. } => "new workspace",
            Self::Settings(_) => "settings",
            Self::Rename { .. } => "rename",
            Self::RemoveWorkspace { .. } => "remove workspace",
            Self::Picker { .. } => "folder picker",
            Self::Issues(_) => "issues",
            Self::Search(_) => "search",
            Self::Update(_) => "update",
            Self::Usage => "usage",
            Self::Remote => "remote",
            Self::Branches(_) => "branches",
            Self::Restart => "restart",
            Self::Keys(_) => "keys",
        }
    }

    pub(super) fn submit_label(&self) -> &'static str {
        match self {
            Self::Rename { .. } => RENAME_SUBMIT,
            Self::RemoveWorkspace { lock: Some(_), .. } => UNLOCK_SUBMIT,
            Self::RemoveWorkspace { check: Check::Changed, .. } => FORCE_REMOVE_SUBMIT,
            Self::RemoveWorkspace { .. } => REMOVE_SUBMIT,
            Self::DeleteGroup { .. } => DELETE_SUBMIT,
            Self::CloseProject { .. }
            | Self::CloseWorkspace { .. }
            | Self::CloseTab { .. }
            | Self::ClosePane { .. } => CLOSE_SUBMIT,
            Self::Update(UpdateStep::Failed(_)) => RETRY_UPDATE_SUBMIT,
            Self::Update(UpdateStep::Installed) | Self::Restart => RESTART_SUBMIT,
            Self::Update(UpdateStep::Manual(_)) => COPY_COMMAND_SUBMIT,
            Self::Update(_) => UPDATE_SUBMIT,
            _ => CREATE_SUBMIT,
        }
    }

    pub(super) fn cancel_label(&self) -> &'static str {
        match self {
            Self::Update(UpdateStep::Installed) => LATER,
            _ => ui::CANCEL_LABEL,
        }
    }

    pub(super) fn input(&mut self) -> Option<&mut String> {
        match self {
            Self::NewWorkspace { input, error, creating: false, .. } => {
                *error = None;
                Some(input)
            }
            Self::Rename { input, .. } | Self::NewGroup { input } => Some(input),
            _ => None,
        }
    }

    pub(super) fn lists_running(&self) -> bool {
        matches!(self, Self::Update(UpdateStep::Installed) | Self::Restart)
    }

    pub(super) fn busy(&self) -> bool {
        matches!(self, Self::NewWorkspace { creating: true, .. })
            || matches!(self, Self::Issues(b) if b.starting)
            || matches!(self, Self::Settings(s) if s.busy())
            || matches!(self, Self::Update(UpdateStep::Updating))
    }
}

impl App {
    pub(super) fn overlay_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.outside_dialog(pos, area) {
            self.cancel_form();
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Picker { .. })) {
            return self.picker_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Search(_))) {
            self.search_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Branches(_))) {
            self.branches_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Issues(_))) {
            return self.issues_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Settings(_))) {
            self.settings_mouse(ev, pos, area);
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Update(_) | Overlay::Restart)) {
            return self.update_mouse(ev, pos, area);
        }
        if matches!(self.overlay, Some(Overlay::Usage)) {
            if ev.kind == MouseEventKind::Down(MouseButton::Left) {
                let view = self.usage_view();
                if ui::usage_done(area, &view).contains(pos) {
                    self.overlay = None;
                } else if let Some(i) = ui::usage_tabs(area, &view).iter().position(|r| r.contains(pos)) {
                    self.switch_usage(if i == 0 { usage::Page::Limits } else { usage::Page::Stats });
                } else if view.page == usage::Page::Stats
                    && !view.stats.empty
                    && ui::usage_share(area, &view).contains(pos)
                {
                    self.host_writes.push(clipboard::osc52(&crate::stats::share_link(&self.stats, SystemTime::now())));
                    self.toast = Some(Toast::new("copied a link to your stats", ui::ToastIcon::Check));
                }
            }
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::Remote)) {
            let done = ui::remote_done(area, &self.remote_view());
            if ev.kind == MouseEventKind::Down(MouseButton::Left) && done.contains(pos) {
                self.overlay = None;
            }
            return Ok(());
        }
        if let Some(Overlay::GroupStyle { group }) = self.overlay {
            self.group_style_mouse(group, ev, pos, area);
            return Ok(());
        }
        if let Some(Overlay::Keys(group)) = self.overlay {
            return self.keys_mouse(group, ev, pos, area);
        }
        let MouseEventKind::Down(button) = ev.kind else { return Ok(()) };
        if let Some(Overlay::Menu { at, actions }) = &self.overlay {
            let at = *at;
            let menu = ui::menu_area(area, at, &self.menu_labels(actions));
            let picked = ui::menu_hit(menu, actions.len(), pos).map(|i| actions[i]);
            self.overlay = None;
            if let (MouseButton::Left, Some(action)) = (button, picked) {
                return self.menu_action(action, at, area);
            }
            return Ok(());
        }
        let Some(overlay) = &self.overlay else { return Ok(()) };
        if button != MouseButton::Left {
            return Ok(());
        }
        let (submit, extra) = self.dialog_labels(overlay);
        match ui::form_hit(area, submit, extra, pos) {
            Some(FormHit::Submit) => self.submit_form(area)?,
            Some(FormHit::Extra) => self.submit_extra(),
            Some(FormHit::Cancel) => self.cancel_form(),
            Some(FormHit::Toggle) => self.toggle_worktree(),
            None => {}
        }
        Ok(())
    }

    pub(super) fn overlay_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        let home = self.home.as_deref();
        let note = |error: &Option<String>| error.clone().map(ui::Note::Error);
        Some(match overlay {
            Overlay::Menu { at, actions } => ui::Overlay::Menu { at: *at, items: self.menu_labels(actions) },
            Overlay::GroupStyle { group } => ui::Overlay::GroupStyle(self.group(*group)?.clone()),
            Overlay::NewGroup { input } => ui::Overlay::Form(ui::Form {
                title: "New group",
                label: "name",
                value: input.clone(),
                hint: NEW_GROUP_HINT.into(),
                toggle: None,
                note: None,
                submit: CREATE_SUBMIT,
            }),
            Overlay::NewWorkspace { project, input, worktree, error, creating } => {
                let repo = self.project_index(*project).map(|p| self.projects[p].path.clone()).unwrap_or_default();
                let path = if *worktree == Some(true) {
                    worktree::checkout_path(&self.config.worktrees_dir(home), &repo, input.trim())
                } else {
                    repo
                };
                ui::Overlay::Form(ui::Form {
                    title: "New workspace",
                    label: "name",
                    value: input.clone(),
                    hint: format!("in {}", crate::text::display_path(&path, home)),
                    toggle: worktree.map(|on| ui::Toggle { label: WORKTREE_TOGGLE, on }),
                    note: if *creating { Some(ui::Note::Busy("creating…")) } else { note(error) },
                    submit: CREATE_SUBMIT,
                })
            }
            Overlay::Settings(s) => ui::Overlay::Settings(settings_view(s)),
            Overlay::Rename { target, input } => ui::Overlay::Form(ui::Form {
                title: target.rename_title(),
                label: "name",
                value: input.clone(),
                hint: target.rename_hint().into(),
                toggle: None,
                note: None,
                submit: RENAME_SUBMIT,
            }),
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.remove_view(*project, *workspace, *check, lock.as_ref(), overlay.submit_label())
            }
            Overlay::DeleteGroup { group } => {
                let (submit, extra) = self.dialog_labels(overlay);
                ui::Overlay::Confirm(ui::Confirm {
                    title: "Delete group",
                    message: self.delete_group_message(*group)?,
                    note: None,
                    submit,
                    extra,
                })
            }
            Overlay::CloseProject { project } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close project",
                message: self.close_project_message(*project)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::CloseWorkspace { project, workspace } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close workspace",
                message: self.close_workspace_message(*project, *workspace)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::CloseTab { project, workspace, tab } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close tab",
                message: self.close_tab_message(*project, *workspace, *tab)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::ClosePane { pane } => ui::Overlay::Confirm(ui::Confirm {
                title: "Close pane",
                message: self.close_pane_message(*pane)?,
                note: None,
                submit: overlay.submit_label(),
                extra: None,
            }),
            Overlay::Keys(group) => ui::Overlay::Keys(self.keys_view(*group)),
            Overlay::Picker { picker, group } => Self::picker_view(picker, group.is_some(), home),
            Overlay::Issues(b) => b.view(area, time::now()),
            Overlay::Search(search) => self.search_view(search),
            Overlay::Update(step) => self.update_view(step, area),
            Overlay::Restart => self.restart_view(area),
            Overlay::Usage => ui::Overlay::Usage(self.usage_view()),
            Overlay::Remote => ui::Overlay::Remote(self.remote_view()),
            Overlay::Branches(picker) => Self::branches_view(picker),
        })
    }
}
