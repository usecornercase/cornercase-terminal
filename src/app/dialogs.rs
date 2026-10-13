use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::menus::PaneAction;
use super::overlay::Overlay;
use crate::error::Result;
use crate::ui::{self};

pub(super) const KEEP_PROJECTS_SUBMIT: &str = "keep projects";

pub(super) const CLOSE_PROJECTS_SUBMIT: &str = "close projects";

impl App {
    pub(super) fn outside_dialog(&self, pos: Position, area: Rect) -> bool {
        let Some(overlay) = &self.overlay else { return false };
        self.overlay_view(overlay, area).and_then(|o| o.area(area)).is_some_and(|r| !r.contains(pos))
    }

    pub(super) fn dialog_labels(&self, overlay: &Overlay) -> (&'static str, Option<&'static str>) {
        match overlay {
            Overlay::DeleteGroup { group } if self.group_projects(*group).next().is_some() => {
                (KEEP_PROJECTS_SUBMIT, Some(CLOSE_PROJECTS_SUBMIT))
            }
            _ => (overlay.submit_label(), None),
        }
    }

    pub(super) fn submit_extra(&mut self) {
        if let Some(Overlay::DeleteGroup { group }) = self.overlay {
            self.overlay = None;
            self.delete_group_and_projects(group);
        }
    }

    pub(super) fn group_style_mouse(&mut self, group: u64, ev: MouseEvent, pos: Position, area: Rect) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let hit = ui::style_hit(area, pos);
        let Some(entry) = self.group_mut(group) else {
            self.overlay = None;
            return;
        };
        match hit {
            Some(ui::StyleHit::Icon(i)) => entry.icon = ui::GROUP_ICONS[i],
            Some(ui::StyleHit::Colour(i)) => entry.colour = ui::GROUP_COLOURS[i],
            Some(ui::StyleHit::Done) => self.overlay = None,
            None => {}
        }
    }

    pub(super) fn toggle_worktree(&mut self) {
        if let Some(Overlay::NewWorkspace { worktree: Some(on), creating: false, .. }) = &mut self.overlay {
            *on = !*on;
        }
    }

    pub(super) fn form_key(&mut self, key: KeyEvent, area: Rect) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_form(),
            KeyCode::Enter => self.submit_form(area)?,
            KeyCode::Tab => self.toggle_worktree(),
            KeyCode::Up => self.scroll_update(-1, area),
            KeyCode::Down => self.scroll_update(1, area),
            KeyCode::Backspace => {
                if let Some(input) = self.overlay.as_mut().and_then(Overlay::input) {
                    input.pop();
                }
            }
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                if let Some(input) = self.overlay.as_mut().and_then(Overlay::input) {
                    input.push(c);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn cancel_form(&mut self) {
        if !self.overlay.as_ref().is_some_and(Overlay::busy) {
            self.overlay = None;
        }
    }

    pub(super) fn submit_form(&mut self, area: Rect) -> Result<()> {
        let Some(overlay) = self.overlay.take() else { return Ok(()) };
        self.overlay = match overlay {
            Overlay::NewWorkspace { project, input, worktree, creating: false, .. } => {
                return self.create_workspace(project, input, worktree, area);
            }
            Overlay::Rename { target, input } => {
                let name = input.trim();
                self.rename(target, (!name.is_empty()).then(|| name.to_string()));
                None
            }
            Overlay::NewGroup { input } if input.trim().is_empty() => Some(Overlay::NewGroup { input }),
            Overlay::NewGroup { input } => {
                self.add_group(input.trim().to_string());
                None
            }
            Overlay::GroupStyle { .. } | Overlay::Usage | Overlay::Remote => None,
            Overlay::RemoveWorkspace { project, workspace, check, lock } => {
                self.confirm_removal(project, workspace, check, lock.is_some())
            }
            Overlay::DeleteGroup { group } => {
                self.delete_group(group);
                None
            }
            Overlay::CloseProject { project } => {
                self.close_project(project);
                None
            }
            Overlay::CloseWorkspace { project, workspace } => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    self.close_workspace(p, w);
                }
                None
            }
            Overlay::CloseTab { project, workspace, tab } => {
                if let Some((p, w, t)) = self.tab_index(project, workspace, tab) {
                    self.close_tab(p, w, t);
                }
                None
            }
            Overlay::ClosePane { pane } => {
                self.pane_action(pane, PaneAction::Close, area)?;
                None
            }
            Overlay::Update(step) => self.submit_update(step),
            Overlay::Restart => {
                self.restart = true;
                None
            }
            busy => Some(busy),
        };
        Ok(())
    }

    pub(super) fn delete_group_message(&self, id: u64) -> Option<String> {
        let message = format!("Delete the group {}?", self.group(id)?.name);
        let (projects, tabs) = self
            .group_projects(id)
            .fold((0, 0), |(n, tabs), p| (n + 1, tabs + p.workspaces.iter().map(|w| w.tabs.len()).sum::<usize>()));
        let (which, them) = match projects {
            0 => return Some(message),
            1 => ("its project".to_string(), "it"),
            n => (format!("its {n} projects"), "them"),
        };
        let stops = match tabs {
            0 => String::new(),
            1 => ", which stops 1 tab and the programs running in it".into(),
            n => format!(", which stops {n} tabs and the programs running in them"),
        };
        Some(format!(
            "{message} Keep {which} open, ungrouped, or close {them} too{stops}. Folders and worktrees stay on disk."
        ))
    }

    pub(super) fn close_project_message(&self, id: u64) -> Option<String> {
        let project = &self.projects[self.project_index(id)?];
        let stopped = stopped_tabs(project.workspaces.iter().map(|w| w.tabs.len()).sum());
        Some(format!("Close the project {}?{stopped} Folders and worktrees stay on disk.", self.project_label(project)))
    }

    pub(super) fn close_workspace_message(&self, project: u64, workspace: u64) -> Option<String> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let workspace = &self.projects[p].workspaces[w];
        Some(format!("Close the workspace {}?{}", workspace.label(), stopped_tabs(workspace.tabs.len())))
    }

    pub(super) fn close_tab_message(&self, project: u64, workspace: u64, tab: u64) -> Option<String> {
        let (p, w, t) = self.tab_index(project, workspace, tab)?;
        let name = self.projects[p].workspaces[w].tabs[t].label(&self.config);
        Some(format!("Close the tab {name}? The programs running in it are stopped."))
    }

    pub(super) fn close_pane_message(&self, pane: u64) -> Option<String> {
        let term = self
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .flat_map(|w| &w.tabs)
            .find_map(|t| t.panes.iter().find(|term| term.id == pane))?;
        let name = term.program(&self.config).unwrap_or_else(|| "?".into());
        Some(format!("Close the pane running {name}? What runs in it is stopped."))
    }
}

fn stopped_tabs(tabs: usize) -> String {
    match tabs {
        0 => String::new(),
        1 => " Its tab and the programs running in it are stopped.".into(),
        n => format!(" Its {n} tabs and the programs running in them are stopped."),
    }
}
