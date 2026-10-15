use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::flow_panel::COPY;
use super::flows_dialog::flows_view;
use super::issues_dialog::{room, view};
use super::menus::MenuAction;
use super::menus::Target;
use super::pull_requests::{COPY_LINK, DONE};
use super::settings_dialog::settings_view;
use super::start_flow_dialog::start_flow_view;
use super::update_dialog::UpdateStep;
use super::worktrees::Check;
use crate::changes::BranchPicker;
use crate::error::Result;
use crate::flows::dialog::Dialog;
use crate::flows::progress::Edge;
use crate::flows::start::Start;
use crate::git::worktree;
use crate::issues::browser::Browser;
use crate::list::Search;
use crate::messenger::Via;
use crate::project::picker::Picker;
use crate::settings::Settings;
use crate::shortcuts::Group as KeysGroup;
use crate::time;
use crate::ui;

pub(super) const CREATE_SUBMIT: &str = "create";

pub(super) const RENAME_SUBMIT: &str = "rename";

pub(super) const REMOVE_SUBMIT: &str = "remove";

pub(super) const DELETE_SUBMIT: &str = "delete";

pub(super) const CLOSE_SUBMIT: &str = "close";

pub(super) const FORCE_REMOVE_SUBMIT: &str = "remove anyway";

pub(super) const UNLOCK_SUBMIT: &str = "unlock and remove";

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
    Flows { dialog: Box<Dialog>, back: Option<Box<Overlay>> },
    StartFlow { start: Box<Start>, back: Option<Box<Overlay>> },
    Rename { target: Target, input: String },
    RemoveWorkspace { project: u64, workspace: u64, check: Check, lock: Option<worktree::Lock> },
    Picker { picker: Picker, group: Option<u64> },
    Issues(Box<Browser>),
    Queue(Box<crate::queue::form::Form>),
    Search(Search),
    Update(UpdateStep),
    Restart,
    Usage,
    Remote,
    Pairing(Via),
    Branches(BranchPicker),
    PullRequest { workspace: u64, scroll: usize },
    FlowMessage { workspace: u64, edge: Edge, to: Option<String>, scroll: usize },
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
            Self::Flows { .. } => "flows",
            Self::StartFlow { .. } => "start flow",
            Self::Rename { .. } => "rename",
            Self::RemoveWorkspace { .. } => "remove workspace",
            Self::Picker { .. } => "folder picker",
            Self::Issues(_) => "issues",
            Self::Queue(_) => "queue",
            Self::Search(_) => "search",
            Self::Update(_) => "update",
            Self::Usage => "usage",
            Self::Remote => "remote",
            Self::Pairing(via) => via.target(),
            Self::Branches(_) => "branches",
            Self::Restart => "restart",
            Self::PullRequest { .. } => "pull request",
            Self::FlowMessage { .. } => "flow message",
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
            Self::PullRequest { .. } => COPY_LINK,
            Self::FlowMessage { .. } => COPY,
            _ => CREATE_SUBMIT,
        }
    }

    pub(super) fn cancel_label(&self) -> &'static str {
        match self {
            Self::Update(UpdateStep::Installed) => LATER,
            Self::PullRequest { .. } | Self::FlowMessage { .. } => DONE,
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
        matches!(self, Self::Queue(form) if form.busy)
            || matches!(self, Self::NewWorkspace { creating: true, .. })
            || matches!(self, Self::Issues(b) if b.starting)
            || matches!(self, Self::Settings(s) if s.busy())
            || matches!(self, Self::Update(UpdateStep::Updating))
            || matches!(self, Self::StartFlow { start, .. } if start.waiting)
    }
}

impl App {
    pub(super) fn overlay_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && self.outside_dialog(pos, area) {
            self.cancel_form();
            return Ok(());
        }
        match &self.overlay {
            Some(Overlay::Menu { .. }) => return self.menu_mouse(ev, pos, area),
            Some(Overlay::Picker { .. }) => return self.picker_mouse(ev, pos, area),
            Some(Overlay::Search(_)) => self.search_mouse(ev, pos, area),
            Some(Overlay::Branches(_)) => self.branches_mouse(ev, pos, area),
            Some(Overlay::Issues(_)) => return self.issues_mouse(ev, pos, area),
            Some(Overlay::Queue(_)) => self.queue_form_mouse(ev, pos, area),
            Some(Overlay::Settings(_)) => self.settings_mouse(ev, pos, area),
            Some(Overlay::Flows { .. }) => self.flows_mouse(ev, pos, area),
            Some(Overlay::StartFlow { .. }) => self.start_flow_mouse(ev, pos, area),
            Some(Overlay::Update(_) | Overlay::Restart) => return self.update_mouse(ev, pos, area),
            Some(Overlay::Usage) => self.usage_mouse(ev, pos, area),
            Some(Overlay::PullRequest { .. }) => self.pull_request_mouse(ev, pos, area),
            Some(Overlay::FlowMessage { .. }) => self.flow_message_mouse(ev, pos, area),
            Some(Overlay::Remote) => self.remote_mouse(ev, pos, area),
            Some(Overlay::Pairing(via)) => self.pairing_mouse(*via, ev, pos, area),
            Some(Overlay::GroupStyle { group }) => self.group_style_mouse(*group, ev, pos, area),
            Some(Overlay::Keys(group)) => return self.keys_mouse(*group, ev, pos, area),
            Some(
                Overlay::NewGroup { .. }
                | Overlay::NewWorkspace { .. }
                | Overlay::Rename { .. }
                | Overlay::RemoveWorkspace { .. }
                | Overlay::DeleteGroup { .. }
                | Overlay::CloseProject { .. }
                | Overlay::CloseWorkspace { .. }
                | Overlay::CloseTab { .. }
                | Overlay::ClosePane { .. },
            ) => return self.form_mouse(ev, pos, area),
            None => (),
        }
        Ok(())
    }

    pub(super) fn overlay_view(&self, overlay: &Overlay, area: Rect) -> Option<ui::Overlay> {
        let home = self.home.as_deref();
        Some(match overlay {
            Overlay::Menu { at, actions } => self.menu_view(*at, actions),
            Overlay::GroupStyle { group } => self.group_style_view(*group)?,
            Overlay::NewGroup { input } => Self::new_group_view(input),
            Overlay::NewWorkspace { project, input, worktree, error, creating } => {
                self.new_workspace_view(*project, input, *worktree, error.as_deref(), *creating)
            }
            Overlay::Settings(s) => ui::Overlay::Settings(settings_view(s)),
            Overlay::Flows { dialog, .. } => flows_view(dialog, area),
            Overlay::StartFlow { start, .. } => start_flow_view(start, area),
            Overlay::Rename { target, input } => Self::rename_view(*target, input),
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.remove_view(*project, *workspace, *check, lock.as_ref(), overlay.submit_label())
            }
            Overlay::DeleteGroup { group } => self.delete_group_view(*group)?,
            Overlay::CloseProject { project } => self.close_project_view(*project)?,
            Overlay::CloseWorkspace { project, workspace } => self.close_workspace_view(*project, *workspace)?,
            Overlay::CloseTab { project, workspace, tab } => self.close_tab_view(*project, *workspace, *tab)?,
            Overlay::ClosePane { pane } => self.close_pane_view(*pane)?,
            Overlay::Keys(group) => ui::Overlay::Keys(self.keys_view(*group)),
            Overlay::Picker { picker, group } => Self::picker_view(picker, group.is_some(), home),
            Overlay::Queue(form) => ui::Overlay::Queue(Self::queue_form_view(form)),
            Overlay::Issues(b) => view(b, room(area), time::now()),
            Overlay::Search(search) => self.search_view(search),
            Overlay::Update(step) => self.update_view(step, area),
            Overlay::Restart => self.restart_view(area),
            Overlay::Usage => ui::Overlay::Usage(self.usage_view()),
            Overlay::Remote => ui::Overlay::Remote(self.remote_view()),
            Overlay::Pairing(via) => ui::Overlay::Remote(self.pairing_view(*via)),
            Overlay::Branches(picker) => Self::branches_view(picker),
            Overlay::PullRequest { workspace, scroll } => self.pull_request_view(*workspace, *scroll)?,
            Overlay::FlowMessage { workspace, edge, to, scroll } => {
                self.flow_message_view(*workspace, (*edge, to.as_deref()), *scroll, area)?
            }
        })
    }
}
