use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::account_menu::AccountItem;
use super::help_menu::HelpItem;
use super::opening::Subject;
use super::overlay::Overlay;
use super::plugins::Spot;
use crate::error::Result;
use crate::host::open::Opener;
use crate::log::{self};
use crate::messenger::Via;
use crate::project::Hierarchy;
use crate::project::group;
use crate::project::split::Dir;
use crate::settings;
use crate::ui::{self};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Group(u64),
    Project(u64),
    Workspace(u64, u64),
    Tab(u64, u64, u64),
}

impl Target {
    fn rename_label(self) -> &'static str {
        match self {
            Self::Group(_) => "rename group",
            Self::Project(_) => "rename project",
            Self::Workspace(..) => "rename workspace",
            Self::Tab(..) => "rename tab",
        }
    }

    pub(super) fn rename_title(self) -> &'static str {
        match self {
            Self::Group(_) => "Rename group",
            Self::Project(_) => "Rename project",
            Self::Workspace(..) => "Rename workspace",
            Self::Tab(..) => "Rename tab",
        }
    }

    pub(super) fn rename_hint(self) -> &'static str {
        match self {
            Self::Group(_) => "leave it empty to keep the current name",
            Self::Project(_) => "leave it empty to use the folder name",
            Self::Workspace(..) => "leave it empty to use the branch name",
            Self::Tab(..) => "leave it empty to use the program name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaneAction {
    Split(Dir),
    RightClicksToPane,
    RightClicksToMenu,
    Close,
}

impl PaneAction {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Split(Dir::Right) => "split right",
            Self::Split(Dir::Down) => "split down",
            Self::RightClicksToPane => "send right-clicks to the pane",
            Self::RightClicksToMenu => "use this menu on right-click",
            Self::Close => "close pane",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Marked {
    Pane(u64),
    Tab(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuAction {
    Rename(Target),
    MoveToGroup(u64),
    SetGroup(u64, Option<u64>),
    GroupStyle(u64),
    DeleteGroup(u64),
    AddProject(u64),
    OpenProject,
    NewGroup,
    Pane(u64, PaneAction),
    Mark(Marked, bool),
    AgentFold(u64, bool),
    CloseAgent(u64),
    CloseAgents(u64),
    ReopenAgents(u64),
    ForgetAgents(u64),
    PullRequest(u64),
    ShowFlow(u64),
    Browser,
    Away(Via),
    NewWorkspace(u64),
    StartFlow(u64),
    ProjectSettings(u64),
    Worktrees(u64),
    RunSetup(u64),
    OpenIn(Subject),
    OpenWith(Subject, Opener),
    CopyPath(Subject),
    SendTodo { todo: u64, pane: u64 },
    Plugin { spot: Spot, index: u16, revision: u32 },
    PluginButton { plugin: u16, revision: u32 },
    Account(AccountItem, (u64, u64)),
    AccountNote(AccountItem),
    AccountsOf(AccountItem),
    AccountMenu(u64),
    ManageAccounts,
    Help(HelpItem),
    Tour(u8),
}

impl App {
    pub(super) fn menu_view(&self, at: Position, actions: &[MenuAction]) -> ui::Overlay {
        let blocker = self.menu_blocker(actions);
        let disabled = actions.iter().map(|&action| Self::account_disabled(action, blocker.is_some())).collect();
        ui::Overlay::Menu { at, items: self.labels_of(actions, blocker.as_deref()), disabled }
    }

    pub(super) fn menu_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let MouseEventKind::Down(button) = ev.kind else { return Ok(()) };
        let Some(Overlay::Menu { at, actions }) = &self.overlay else { return Ok(()) };
        let at = *at;
        let menu = ui::menu_area(area, at, &self.menu_labels(actions));
        let picked = ui::menu_hit(menu, actions.len(), pos).map(|i| actions[i]);
        self.overlay = None;
        if let (MouseButton::Left, Some(action)) = (button, picked) {
            return self.menu_action(action, at, area);
        }
        Ok(())
    }

    pub(super) fn menu_labels(&self, actions: &[MenuAction]) -> Vec<String> {
        self.labels_of(actions, self.menu_blocker(actions).as_deref())
    }

    fn labels_of(&self, actions: &[MenuAction], blocker: Option<&str>) -> Vec<String> {
        actions.iter().map(|&a| self.menu_label(a, blocker)).collect()
    }

    fn concern_menu_label(&self, action: MenuAction, blocker: Option<&str>) -> Option<String> {
        self.account_menu_label(action, blocker)
            .or_else(|| self.help_menu_label(action))
            .or_else(|| self.closed_agents_label(action))
    }

    fn menu_label(&self, action: MenuAction, blocker: Option<&str>) -> String {
        if let Some(label) = self.concern_menu_label(action, blocker) {
            return label;
        }
        match action {
            MenuAction::Rename(target) => target.rename_label().into(),
            MenuAction::MoveToGroup(_) => "move to group".into(),
            MenuAction::SetGroup(_, None) => "no group".into(),
            MenuAction::SetGroup(_, Some(id)) => self.group(id).map(group::GroupEntry::label).unwrap_or_default(),
            MenuAction::GroupStyle(_) => "icon and colour".into(),
            MenuAction::DeleteGroup(_) => "delete group".into(),
            MenuAction::AddProject(_) => "add project".into(),
            MenuAction::OpenProject => "open project".into(),
            MenuAction::NewGroup => "new group".into(),
            MenuAction::NewWorkspace(_) => "new workspace".into(),
            MenuAction::StartFlow(_) => "start a flow…".into(),
            MenuAction::ProjectSettings(_) => "project settings".into(),
            MenuAction::Worktrees(_) => "worktrees".into(),
            MenuAction::RunSetup(_) => "run setup".into(),
            MenuAction::OpenIn(_) => "open in…".into(),
            MenuAction::OpenWith(_, opener) => opener.name().into(),
            MenuAction::CopyPath(_) => "copy path".into(),
            MenuAction::Pane(_, action) => action.label().into(),
            MenuAction::Mark(_, true) => "mark as pending review".into(),
            MenuAction::Mark(_, false) => "mark as reviewed".into(),
            MenuAction::AgentFold(_, true) => "collapse".into(),
            MenuAction::AgentFold(_, false) => "expand".into(),
            MenuAction::CloseAgent(_) => "close agent".into(),
            MenuAction::PullRequest(workspace) => self.pull_request_label(workspace),
            MenuAction::ShowFlow(_) => "show flow".into(),
            MenuAction::Browser if self.config.remote => "open in your phone's browser".into(),
            MenuAction::Browser => "set up your phone's browser…".into(),
            MenuAction::Away(via) if !self.messenger(via).ready() => format!("set up {}…", via.name()),
            MenuAction::Away(via) if self.messenger(via).away => format!("stop messaging on {}", via.name()),
            MenuAction::Away(via) => format!("continue on {}", via.name()),
            MenuAction::SendTodo { pane, .. } => self.todo_agent_label(pane),
            MenuAction::Plugin { index, .. } => self.plugin_label(index),
            MenuAction::PluginButton { plugin, .. } => self.plugin_button_label(plugin),
            _ => String::new(),
        }
    }

    pub(super) fn target_menu(&self, target: Target) -> Vec<MenuAction> {
        let more = match target {
            Target::Tab(_, _, tab) => self.mark_action(Marked::Tab(tab)),
            Target::Workspace(_, workspace) => self.pull_request(workspace).map(|_| MenuAction::PullRequest(workspace)),
            Target::Group(_) | Target::Project(_) => None,
        };
        let account = match target {
            Target::Tab(_, _, tab) => self.account_entry(self.tab_pane(tab)),
            _ => None,
        };
        let flow = match target {
            Target::Workspace(_, workspace) => self.run_of(workspace).map(|_| MenuAction::ShowFlow(workspace)),
            _ => None,
        };
        let closed = match target {
            Target::Workspace(_, workspace) => self.closed_agents_menu(workspace),
            _ => Vec::new(),
        };
        let spot = match target {
            Target::Group(_) => None,
            Target::Project(project) => Some(Spot::Project(project)),
            Target::Workspace(project, workspace) => Some(Spot::Workspace(project, workspace)),
            Target::Tab(project, workspace, tab) => Some(Spot::Tab(project, workspace, tab)),
        };
        let setup = match target {
            Target::Workspace(project, workspace) => self.setup_action(project, workspace),
            _ => None,
        };
        let open = match target {
            Target::Workspace(_, workspace) => Some(self.open_entry(Subject::Workspace(workspace))),
            _ => None,
        };
        std::iter::once(MenuAction::Rename(target))
            .chain(more)
            .chain(account)
            .chain(flow)
            .chain(closed)
            .chain(setup)
            .chain(open)
            .chain(spot.map(|spot| self.plugin_menu(spot)).unwrap_or_default())
            .collect()
    }

    pub(super) fn menu_action(&mut self, action: MenuAction, at: Position, area: Rect) -> Result<()> {
        log::info!("ui", "menu", action = format!("{action:?}"));
        let Some(action) = self.group_action(action, at) else { return Ok(()) };
        let Some(action) = self.project_action(action, at, area) else { return Ok(()) };
        let Some(action) = self.agent_action(action) else { return Ok(()) };
        match action {
            MenuAction::Pane(pane, action) => self.pane_action(pane, action, area),
            _ => Ok(()),
        }
    }

    pub(in crate::app) fn group_action(&mut self, action: MenuAction, at: Position) -> Option<MenuAction> {
        match action {
            MenuAction::Rename(target) => {
                self.overlay = self.current_name(target).map(|input| Overlay::Rename { target, input });
            }
            MenuAction::MoveToGroup(project) => self.move_to_group_menu(project, at),
            MenuAction::SetGroup(project, group) => {
                if let Some(p) = self.projects.project_index(project) {
                    self.projects[p].group = group;
                    self.cancel_pending_group_deletion(group, self.projects[p].closing);
                }
            }
            MenuAction::GroupStyle(group) => self.overlay = Some(Overlay::GroupStyle { group }),
            MenuAction::DeleteGroup(group) => self.overlay = Some(Overlay::DeleteGroup { group }),
            MenuAction::AddProject(group) => self.pick_project(Some(group)),
            MenuAction::OpenProject => self.pick_project(None),
            MenuAction::NewGroup => {
                self.screen.nav = None;
                self.overlay = Some(Overlay::NewGroup { input: String::new() });
            }
            MenuAction::NewWorkspace(project) => {
                if let Some(p) = self.projects.project_index(project) {
                    self.screen.nav = None;
                    self.ask_new_workspace(p);
                }
            }
            other => return Some(other),
        }
        None
    }

    fn move_to_group_menu(&mut self, project: u64, at: Position) {
        let Some(p) = self.projects.project_index(project) else { return };
        let current = self.projects[p].group;
        let mut actions: Vec<MenuAction> = self
            .groups
            .iter()
            .filter(|g| Some(g.id) != current)
            .map(|g| MenuAction::SetGroup(project, Some(g.id)))
            .collect();
        if current.is_some() {
            actions.push(MenuAction::SetGroup(project, None));
        }
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn pick_project(&mut self, group: Option<u64>) {
        self.screen.nav = None;
        self.open_picker(group);
    }

    pub(in crate::app) fn project_action(
        &mut self,
        action: MenuAction,
        at: Position,
        area: Rect,
    ) -> Option<MenuAction> {
        match action {
            MenuAction::StartFlow(project) => self.open_start_flow(self.start_flow_in(project)),
            MenuAction::ProjectSettings(project) => self.open_project_settings(project),
            MenuAction::Worktrees(project) => self.open_worktrees(Some(project)),
            MenuAction::RunSetup(workspace) => self.run_setup(workspace, area),
            MenuAction::OpenIn(subject) => self.open_in_menu(subject, at),
            MenuAction::OpenWith(subject, opener) => self.open_with(subject, opener),
            MenuAction::CopyPath(subject) => self.copy_path(subject),
            MenuAction::PullRequest(workspace) => self.open_pull_request(workspace),
            MenuAction::ShowFlow(workspace) => self.show_flow_of(workspace),
            MenuAction::CloseAgents(workspace) => self.ask_close_agents(workspace),
            MenuAction::ReopenAgents(workspace) => self.reopen_from_window(workspace, area),
            MenuAction::ForgetAgents(workspace) => self.ask_forget_agents(workspace),
            MenuAction::AccountMenu(pane) => self.open_account_submenu(pane, at),
            MenuAction::AccountsOf(item) => self.open_accounts_of(item, at),
            MenuAction::Help(item) => self.help_action(item, at),
            MenuAction::Tour(index) => self.start_tour(usize::from(index), None, true),
            MenuAction::PluginButton { plugin, revision } => self.press_plugin_button(plugin, revision, at),
            other => return Some(other),
        }
        None
    }

    pub(in crate::app) fn agent_action(&mut self, action: MenuAction) -> Option<MenuAction> {
        match action {
            MenuAction::Mark(marked, pending) => {
                self.mark(marked, pending);
            }
            MenuAction::AgentFold(pane, collapsed) => {
                self.set_agent_fold(pane, collapsed);
            }
            MenuAction::CloseAgent(pane) => self.ask_close_agent(pane),
            MenuAction::Browser if self.config.remote => self.open_remote(),
            MenuAction::Browser => self.open_settings_on(&settings::Row::Remote),
            MenuAction::Away(via) => self.toggle_away(via),
            MenuAction::SendTodo { todo, pane } => self.send_todo_from_panel(todo, pane, std::time::Instant::now()),
            MenuAction::Plugin { spot, index, revision } => self.run_plugin_action(spot, index, revision),
            MenuAction::Account(item, revision) => self.account_item(item, revision),
            MenuAction::AccountNote(_) => {}
            MenuAction::ManageAccounts => self.manage_accounts(),
            other => return Some(other),
        }
        None
    }
}
