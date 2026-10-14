use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::overlay::Overlay;
use crate::error::Result;
use crate::log::{self};
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
    Browser,
    Away,
}

impl App {
    pub(super) fn menu_view(&self, at: Position, actions: &[MenuAction]) -> ui::Overlay {
        ui::Overlay::Menu { at, items: self.menu_labels(actions) }
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
        actions.iter().map(|&a| self.menu_label(a)).collect()
    }

    fn menu_label(&self, action: MenuAction) -> String {
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
            MenuAction::Pane(_, action) => action.label().into(),
            MenuAction::Mark(_, true) => "mark as pending review".into(),
            MenuAction::Mark(_, false) => "mark as reviewed".into(),
            MenuAction::Browser if self.config.remote => "open in your phone's browser".into(),
            MenuAction::Browser => "set up your phone's browser…".into(),
            MenuAction::Away if !self.telegram.ready() => "set up Telegram…".into(),
            MenuAction::Away if self.telegram.away => "stop messaging on Telegram".into(),
            MenuAction::Away => "continue on Telegram".into(),
        }
    }

    pub(super) fn target_menu(&self, target: Target) -> Vec<MenuAction> {
        let mark = match target {
            Target::Tab(_, _, tab) => self.mark_action(Marked::Tab(tab)),
            _ => None,
        };
        std::iter::once(MenuAction::Rename(target)).chain(mark).collect()
    }

    pub(super) fn menu_action(&mut self, action: MenuAction, at: Position, area: Rect) -> Result<()> {
        log::info!("ui", "menu", action = format!("{action:?}"));
        match action {
            MenuAction::Rename(target) => {
                self.overlay = self.current_name(target).map(|input| Overlay::Rename { target, input });
            }
            MenuAction::MoveToGroup(project) => {
                let Some(p) = self.projects.project_index(project) else { return Ok(()) };
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
            MenuAction::SetGroup(project, group) => {
                if let Some(p) = self.projects.project_index(project) {
                    self.projects[p].group = group;
                    self.cancel_pending_group_deletion(group, self.projects[p].closing);
                }
            }
            MenuAction::GroupStyle(group) => self.overlay = Some(Overlay::GroupStyle { group }),
            MenuAction::DeleteGroup(group) => self.overlay = Some(Overlay::DeleteGroup { group }),
            MenuAction::AddProject(group) => {
                self.nav = None;
                self.open_picker(Some(group));
            }
            MenuAction::OpenProject => {
                self.nav = None;
                self.open_picker(None);
            }
            MenuAction::NewGroup => {
                self.nav = None;
                self.overlay = Some(Overlay::NewGroup { input: String::new() });
            }
            MenuAction::Pane(pane, action) => return self.pane_action(pane, action, area),
            MenuAction::Mark(marked, pending) => {
                self.mark(marked, pending);
            }
            MenuAction::Browser if self.config.remote => self.open_remote(),
            MenuAction::Browser => self.open_settings_on(&settings::Row::Remote),
            MenuAction::Away => self.toggle_away(),
        }
        Ok(())
    }
}
