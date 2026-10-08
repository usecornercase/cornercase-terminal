use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{App, Overlay, Target, Toast};
use crate::activity::Status;
use crate::error::Result;
use crate::files::Mode;
use crate::log;
use crate::project::Project;
use crate::search::{Goto, Search};
use crate::shortcuts::{self, Action, Group, Step};
use crate::split::{self, Place};
use crate::todo::Field;
use crate::ui::{self, SidebarRow, keys};

const NO_AGENT_WAITS: &str = "no agent needs you";
const NO_ROOM: &str = "no room to split this pane";
const NOT_GIT: &str = "this workspace is not in a git repository";

impl App {
    pub(super) fn is_prefix(&self, key: KeyEvent) -> bool {
        self.config.prefix().is_some_and(|prefix| prefix.matches(key))
    }

    pub(super) fn keys_key(&mut self, group: Option<Group>, key: KeyEvent, area: Rect) -> Result<()> {
        self.overlay = None;
        if self.is_prefix(key) {
            self.forward_key(key);
            return Ok(());
        }
        if key.code == KeyCode::Esc {
            return Ok(());
        }
        self.step(shortcuts::lookup(group, key), area)
    }

    pub(super) fn keys_mouse(&mut self, group: Option<Group>, ev: MouseEvent, pos: Position, area: Rect) -> Result<()> {
        let MouseEventKind::Down(button) = ev.kind else { return Ok(()) };
        let view = self.keys_view(group);
        let menu = keys::area(keys::frame(self.layout(area).shown(self.nav).pane, area, &view), &view);
        let picked = keys::hit(menu, &view, pos).and_then(|i| shortcuts::items(group)[i].step);
        self.overlay = None;
        if button == MouseButton::Left {
            return self.step(picked, area);
        }
        Ok(())
    }

    fn step(&mut self, step: Option<Step>, area: Rect) -> Result<()> {
        match step {
            Some(Step::Open(group)) => self.overlay = Some(Overlay::Keys(Some(group))),
            Some(Step::Run(action)) => return self.shortcut(action, area),
            None => {}
        }
        Ok(())
    }

    pub(super) fn keys_view(&self, group: Option<Group>) -> keys::Keys {
        let items = shortcuts::items(group).iter().map(keys::Item::from).collect();
        let prefix = self.config.prefix().map(|p| p.to_string()).unwrap_or_default();
        let title = group.map_or_else(|| "keys".to_string(), |g| format!("keys › {}", g.name()));
        keys::Keys { title, items, hint: format!("esc closes · {prefix} twice types it in the pane") }
    }

    fn shortcut(&mut self, action: Action, area: Rect) -> Result<()> {
        log::info!("ui", "shortcut", action = format!("{action:?}"));
        match action {
            Action::NextTab => self.step_tab(1),
            Action::PreviousTab => self.step_tab(-1),
            Action::Tab(t) => self.go_to_tab(t),
            Action::NextWorkspace => self.step_workspace(1),
            Action::PreviousWorkspace => self.step_workspace(-1),
            Action::NextProject => self.step_project(1),
            Action::PreviousProject => self.step_project(-1),
            Action::Pane(side) => self.focus_beside(side, area),
            Action::Agent => self.next_agent(),
            Action::Search => {
                self.nav = None;
                self.overlay = Some(Overlay::Search(Search::default()));
            }
            Action::NewTab => {
                if let Some((p, w)) = self.open_workspace_index() {
                    self.nav = None;
                    return self.add_tab(p, w, area);
                }
            }
            Action::Split(dir) => {
                let pane = self.layout(area).pane;
                let Some((tab, id)) = self.tab().and_then(|t| Some((t, t.pane()?.id))) else { return Ok(()) };
                if tab.can_split(pane, id, dir) {
                    return self.split_pane(id, dir, area, true).map(drop);
                }
                self.toast = Some(Toast::new(NO_ROOM, ui::ToastIcon::Bug));
            }
            Action::ClosePane => self.ask_close_pane(),
            Action::RenameTab => {
                let found = self.open_workspace_index().and_then(|(p, w)| {
                    let workspace = &self.projects[p].workspaces[w];
                    (!workspace.tabs.is_empty()).then_some((p, w, workspace.active))
                });
                self.ask_rename(self.tab_target(found));
            }
            Action::FindNames => self.find_files(Mode::Name),
            Action::FindText => self.find_files(Mode::Text),
            Action::Files => self.toggle_files(),
            Action::Changes => self.changes_by_key(),
            Action::Base => self.compare_base(),
            Action::NewWorkspace => {
                if self.project().is_some() {
                    self.nav = None;
                    self.ask_new_workspace(self.active);
                }
            }
            Action::RenameWorkspace => {
                let target = self.open_workspace_index().map(|(p, w)| {
                    let project = &self.projects[p];
                    Target::Workspace(project.id, project.workspaces[w].id)
                });
                self.ask_rename(target);
            }
            Action::CloseWorkspace => self.ask_close_workspace(),
            Action::Issues => {
                if self.issues_available() {
                    self.nav = None;
                    return self.open_issues(area);
                }
            }
            Action::Todo => {
                let opening = !self.todo.open;
                self.toggle_todo();
                if opening {
                    self.todo.field = Some(Field::default());
                }
            }
            Action::Settings => {
                self.nav = None;
                self.open_settings();
            }
            Action::Usage => {
                self.nav = None;
                self.open_usage();
            }
            Action::Quit => self.detach = true,
        }
        Ok(())
    }

    fn open_workspace_index(&self) -> Option<(usize, usize)> {
        let project = self.project()?;
        project.workspace().filter(|w| w.open()).map(|_| (self.active, project.active))
    }

    fn ask_rename(&mut self, target: Option<Target>) {
        if let Some((target, input)) = target.and_then(|t| Some((t, self.current_name(t)?))) {
            self.nav = None;
            self.overlay = Some(Overlay::Rename { target, input });
        }
    }

    fn step_tab(&mut self, delta: isize) {
        let Some(workspace) = self.project_mut().and_then(Project::workspace_mut) else { return };
        if let Some(t) = cycle(workspace.active, workspace.tabs.len(), delta) {
            workspace.active = t;
        }
    }

    fn go_to_tab(&mut self, t: usize) {
        if let Some(workspace) = self.project_mut().and_then(Project::workspace_mut).filter(|w| t < w.tabs.len()) {
            workspace.active = t;
        }
    }

    fn step_workspace(&mut self, delta: isize) {
        let Some(project) = self.project() else { return };
        let open: Vec<usize> = (0..project.workspaces.len()).filter(|&w| project.workspaces[w].open()).collect();
        let at = open.iter().position(|&w| w == project.active).unwrap_or(0);
        let Some(next) = cycle(at, open.len(), delta).map(|i| open[i]) else { return };
        let goto = Goto::Place { project: project.id, workspace: Some(project.workspaces[next].id), tab: None };
        self.goto(goto);
    }

    fn step_project(&mut self, delta: isize) {
        let unfolded = vec![false; self.groups.len()];
        let order: Vec<usize> = ui::sidebar_rows(&self.project_groups(), &unfolded)
            .into_iter()
            .filter_map(|row| if let SidebarRow::Project(p) = row { Some(p) } else { None })
            .filter(|&p| !self.projects[p].closing)
            .collect();
        let at = order.iter().position(|&p| p == self.active).unwrap_or(0);
        if let Some(p) = cycle(at, order.len(), delta).map(|i| order[i]) {
            self.goto(Goto::Place { project: self.projects[p].id, workspace: None, tab: None });
        }
    }

    fn focus_beside(&mut self, side: Place, area: Rect) {
        let pane = self.layout(area).pane;
        let Some(tab) = self.tab_mut() else { return };
        let shown = tab.shown(pane);
        let beside = tab.pane().and_then(|t| split::beside(&shown, t.id, side));
        if let Some(id) = beside {
            tab.focus(id);
        }
    }

    fn next_agent(&mut self) {
        let places = self.agent_places();
        let pane = |&(p, w, t, i): &(usize, usize, usize, usize)| &self.projects[p].workspaces[w].tabs[t].panes[i];
        let here = self.term().map(|t| t.id);
        let at = places.iter().position(|place| Some(pane(place).id) == here);
        let count = places.len();
        let start = at.map_or(0, |at| at + 1);
        let next = (0..count)
            .map(|k| &places[(start + k) % count])
            .find(|place| pane(place).agent.status().is_some_and(Status::needs_you))
            .map(|place| pane(place).id);
        match next {
            Some(id) => self.jump_to_pane(id),
            None => self.toast = Some(Toast::new(NO_AGENT_WAITS, ui::ToastIcon::Check)),
        }
    }

    fn ask_close_pane(&mut self) {
        let Some(project) = self.project() else { return };
        let Some(workspace) = project.workspace() else { return };
        let Some(tab) = workspace.tab() else { return };
        let Some(pane) = tab.pane() else { return };
        let overlay = if tab.panes.len() == 1 {
            Overlay::CloseTab { project: project.id, workspace: workspace.id, tab: tab.id }
        } else {
            Overlay::ClosePane { pane: pane.id }
        };
        self.nav = None;
        self.overlay = Some(overlay);
    }

    fn ask_close_workspace(&mut self) {
        let Some((p, w)) = self.open_workspace_index() else { return };
        self.nav = None;
        let project = &self.projects[p];
        if project.workspaces[w].worktree {
            self.ask_removal(p, w);
        } else {
            self.overlay = Some(Overlay::CloseWorkspace { project: project.id, workspace: project.workspaces[w].id });
        }
    }

    fn find_files(&mut self, mode: Mode) {
        let Some(workspace) = self.project().and_then(Project::workspace).map(|w| w.id) else { return };
        if !self.files.open {
            self.toggle_files();
        }
        self.files.search(workspace, mode);
    }

    fn changes_by_key(&mut self) {
        if self.changes_target().is_none() {
            self.toast = Some(Toast::new(NOT_GIT, ui::ToastIcon::Bug));
            return;
        }
        let opening = !self.changes.open;
        self.toggle_changes();
        if opening {
            self.changes.filter.get_or_insert_default().focused = true;
        }
    }

    fn compare_base(&mut self) {
        let Some(target) = self.changes_target() else {
            self.toast = Some(Toast::new(NOT_GIT, ui::ToastIcon::Bug));
            return;
        };
        if !self.changes.open {
            self.toggle_changes();
        }
        self.open_branches(&target);
    }
}

fn cycle(at: usize, len: usize, delta: isize) -> Option<usize> {
    let len = isize::try_from(len).ok().filter(|&l| l > 0)?;
    let at = isize::try_from(at).ok()?;
    usize::try_from((at + delta).rem_euclid(len)).ok()
}
