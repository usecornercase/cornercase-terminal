use std::collections::HashSet;
use std::path::PathBuf;

use super::App;
use super::menus::Target;
use super::overlay::Overlay;
use crate::log::{self};
use crate::project::search::Goto;
use crate::project::{Group, Hierarchy, Phase, Project, Tab, Workspace, group, move_before, shift_active};
use crate::term::Term;

impl App {
    pub(super) fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(super) fn project(&self) -> Option<&Project> {
        self.projects.get(self.active)
    }

    pub(super) fn project_mut(&mut self) -> Option<&mut Project> {
        self.projects.get_mut(self.active)
    }

    pub(super) fn tab(&self) -> Option<&Tab> {
        self.project().and_then(Project::workspace).and_then(Workspace::tab)
    }

    pub(super) fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.project_mut().and_then(Project::workspace_mut).and_then(Workspace::tab_mut)
    }

    pub(super) fn term(&self) -> Option<&Term> {
        self.tab().and_then(Tab::pane)
    }

    pub(super) fn term_mut(&mut self) -> Option<&mut Term> {
        self.tab_mut().and_then(Tab::pane_mut)
    }

    pub(super) fn pane_ids(&self) -> HashSet<u64> {
        self.projects.terms().map(|term| term.id).collect()
    }

    pub(super) fn live_parent(term: &Term, live: &HashSet<u64>) -> Option<u64> {
        term.parent.filter(|id| live.contains(id))
    }

    pub(super) fn group_index(&self, id: u64) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    pub(super) fn group(&self, id: u64) -> Option<&group::GroupEntry> {
        self.groups.iter().find(|g| g.id == id).map(|g| &g.entry)
    }

    pub(super) fn group_mut(&mut self, id: u64) -> Option<&mut group::GroupEntry> {
        self.groups.iter_mut().find(|g| g.id == id).map(|g| &mut g.entry)
    }

    pub(super) fn project_label(&self, project: &Project) -> String {
        project.label(self.home.as_deref())
    }

    pub(super) fn remove(&mut self, id: u64) {
        if let Some(term) = self.projects.pane_by_mut(id) {
            let status = term.exit_status().unwrap_or_else(|| "unknown".into());
            log::info!("app", "shell exited", pane = id, status = status);
        }
        let Some(p) = self.projects.iter_mut().position(|p| p.remove_term(id)) else { return };
        if self.projects[p].closing && !self.projects[p].has_terms() {
            self.remove_project(p);
        }
        if self.closing_gone() {
            self.overlay = None;
        }
    }

    fn closing_gone(&self) -> bool {
        match self.overlay {
            Some(Overlay::CloseProject { project }) => self.projects.project_index(project).is_none(),
            Some(Overlay::CloseWorkspace { project, workspace }) => {
                self.projects.workspace_in(project, workspace).is_none()
            }
            Some(Overlay::CloseTab { project, workspace, tab }) => {
                self.projects.tab_in(project, workspace, tab).is_none()
            }
            _ => false,
        }
    }

    pub(super) fn reap(&mut self) {
        let exited: Vec<u64> = self.projects.terms_mut().filter_map(|t| t.exited().then_some(t.id)).collect();
        for id in exited {
            self.remove(id);
        }
    }

    pub(super) fn remove_project(&mut self, p: usize) {
        self.forget_project_changes(p);
        self.projects.remove(p);
        shift_active(&mut self.active, p);
        let projects = &self.projects;
        let empty = |id: &u64| !projects.iter().any(|p| p.group == Some(*id));
        let gone: Vec<u64> = self.deleting_groups.iter().copied().filter(empty).collect();
        for id in gone {
            self.delete_group(id);
        }
    }

    fn forget_project_changes(&mut self, p: usize) {
        let project = &self.projects[p];
        for workspace in &project.workspaces {
            self.changes.forget(workspace.id);
        }
        self.changes.headers.remove(&project.id);
    }

    pub(super) fn close_project(&mut self, id: u64) {
        let Some(p) = self.projects.project_index(id) else { return };
        self.forget_project_changes(p);
        let project = &mut self.projects[p];
        let dirs: Vec<(u64, Vec<PathBuf>)> =
            project.workspaces.iter_mut().map(|w| (w.id, std::mem::take(&mut w.run_dirs))).collect();
        project.closing = true;
        project.kill();
        if !project.has_terms() {
            self.remove_project(p);
        }
        for (workspace, dirs) in dirs {
            self.defer_run_dirs(workspace, dirs);
        }
    }

    pub(super) fn set_collapsed(&mut self, target: Target, collapsed: bool) {
        match target {
            Target::Group(id) => {
                if let Some(entry) = self.group_mut(id) {
                    entry.collapsed = collapsed;
                }
            }
            Target::Project(id) => {
                if let Some(p) = self.projects.project_index(id) {
                    self.projects[p].collapsed = collapsed;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.projects.workspace_in(project, workspace) {
                    self.projects[p].workspaces[w].collapsed = collapsed;
                }
            }
            Target::Tab(..) => {}
        }
    }

    pub(super) fn toggle_fold(&mut self, target: Target) {
        let collapsed = match target {
            Target::Project(id) => self.projects.project_index(id).map(|p| self.projects[p].collapsed),
            Target::Workspace(project, workspace) => {
                self.projects.workspace_in(project, workspace).map(|(p, w)| self.projects[p].workspaces[w].collapsed)
            }
            Target::Group(_) | Target::Tab(..) => return self.click_row(target),
        };
        if let Some(collapsed) = collapsed {
            self.set_collapsed(target, !collapsed);
        }
    }

    pub(super) fn close_tab(&mut self, p: usize, w: usize, t: usize) {
        for term in &mut self.projects[p].workspaces[w].tabs[t].panes {
            term.kill();
        }
    }

    pub(super) fn move_project(&mut self, id: u64, group: Option<u64>, before: Option<u64>) {
        let active = self.project().map(|p| p.id);
        let Some(p) = self.projects.project_index(id) else { return };
        let mut project = self.projects.remove(p);
        project.group = group;
        self.cancel_pending_group_deletion(group, project.closing);
        let at = before.and_then(|q| self.projects.project_index(q)).unwrap_or_else(|| {
            self.projects.iter().rposition(|p| p.group == group).map_or(self.projects.len(), |i| i + 1)
        });
        self.projects.insert(at, project);
        self.active = active.and_then(|id| self.projects.project_index(id)).unwrap_or(0);
    }

    pub(super) fn move_group(&mut self, group: usize, before: usize) {
        move_before(&mut self.groups, group, before, None);
    }

    pub(super) fn move_workspace(&mut self, p: usize, w: usize, before: usize) {
        let project = &mut self.projects[p];
        move_before(&mut project.workspaces, w, before, Some(&mut project.active));
    }

    pub(super) fn move_tab(&mut self, p: usize, w: usize, t: usize, before: usize) {
        let workspace = &mut self.projects[p].workspaces[w];
        move_before(&mut workspace.tabs, t, before, Some(&mut workspace.active));
    }

    pub(super) fn style_group(&mut self, id: u64, icon: Option<char>, colour: Option<u8>) -> bool {
        let Some(entry) = self.group_mut(id) else { return false };
        if let Some(icon) = icon {
            entry.icon = icon;
        }
        if let Some(colour) = colour {
            entry.colour = colour;
        }
        true
    }

    pub(super) fn close_workspace(&mut self, p: usize, w: usize) {
        if self.projects[p].workspaces[w].worktree {
            self.ask_removal(p, w);
            return;
        }
        self.drop_workspace(self.projects[p].id, self.projects[p].workspaces[w].id);
    }

    pub(super) fn goto(&mut self, goto: Goto) {
        self.nav = None;
        self.files.set_full_screen(false);
        let (project, workspace, tab) = match goto {
            Goto::Group(id) => {
                if let Some(entry) = self.group_mut(id) {
                    entry.collapsed = false;
                }
                let Some(first) = self.projects.iter().find(|p| p.group == Some(id)) else { return };
                (first.id, None, None)
            }
            Goto::Place { project, workspace, tab } => (project, workspace, tab),
        };
        let Some(p) = self.projects.project_index(project) else { return };
        self.active = p;
        let project = &mut self.projects[p];
        if let Some(w) = workspace.and_then(|id| project.index_of(id)) {
            project.active = w;
            let workspace = &mut project.workspaces[w];
            if let Some(t) = tab.and_then(|id| workspace.index_of(id)) {
                workspace.active = t;
            }
        }
        self.unfold_focus();
    }

    pub(super) fn current_name(&self, target: Target) -> Option<String> {
        match target {
            Target::Group(id) => self.group(id).map(|g| g.name.clone()),
            Target::Project(id) => self.projects.project_index(id).map(|p| self.project_label(&self.projects[p])),
            Target::Workspace(project, workspace) => {
                self.projects.workspace_in(project, workspace).map(|(p, w)| self.projects[p].workspaces[w].label())
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w, t) = self.projects.tab_in(project, workspace, tab)?;
                Some(self.projects[p].workspaces[w].tabs[t].label(&self.config))
            }
        }
    }

    pub(super) fn rename(&mut self, target: Target, name: Option<String>) {
        log::info!("app", "rename", target = format!("{target:?}"), name = name.as_deref().unwrap_or("-"));
        match target {
            Target::Group(id) => {
                if let Some(entry) = self.group_mut(id)
                    && let Some(name) = name
                {
                    entry.name = name;
                }
            }
            Target::Project(id) => {
                if let Some(p) = self.projects.project_index(id) {
                    self.projects[p].name = name;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.projects.workspace_in(project, workspace) {
                    self.projects[p].workspaces[w].name = name;
                }
            }
            Target::Tab(project, workspace, tab) => {
                if let Some((p, w, t)) = self.projects.tab_in(project, workspace, tab) {
                    self.projects[p].workspaces[w].tabs[t].name = name;
                }
            }
        }
    }

    pub(super) fn delete_group(&mut self, id: u64) {
        self.groups.retain(|g| g.id != id);
        self.deleting_groups.retain(|&g| g != id);
        if self.overlay.as_ref().is_some_and(|overlay| overlay.names_group(id)) {
            self.overlay = None;
        }
        for p in self.projects.iter_mut().filter(|p| p.group == Some(id)) {
            p.group = None;
        }
    }

    pub(super) fn cancel_pending_group_deletion(&mut self, group: Option<u64>, closing: bool) {
        if !closing {
            self.deleting_groups.retain(|&id| Some(id) != group);
        }
    }

    pub(super) fn delete_group_and_projects(&mut self, id: u64) {
        let projects: Vec<u64> = self.group_projects(id).map(|p| p.id).collect();
        for project in projects {
            self.close_project(project);
        }
        if self.projects.iter().any(|p| p.group == Some(id)) {
            self.deleting_groups.push(id);
        } else {
            self.delete_group(id);
        }
    }

    pub(super) fn group_projects(&self, id: u64) -> impl Iterator<Item = &Project> {
        self.projects.iter().filter(move |p| p.group == Some(id) && !p.closing)
    }

    pub(super) fn add_group(&mut self, name: String) -> u64 {
        let (icon, colour) = group::preset(&self.groups);
        let id = self.take_id();
        self.groups.push(Group { id, entry: group::GroupEntry { name, icon, colour, collapsed: false } });
        id
    }

    pub(super) fn removing(&self, p: usize, w: usize) -> bool {
        self.projects.get(p).and_then(|p| p.workspaces.get(w)).is_some_and(Workspace::removing)
    }

    pub(super) fn drop_workspace(&mut self, project: u64, workspace: u64) {
        let Some((p, w)) = self.projects.workspace_in(project, workspace) else { return };
        self.changes.forget(workspace);
        let workspace = &mut self.projects[p].workspaces[w];
        let dirs = std::mem::take(&mut workspace.run_dirs);
        let id = workspace.id;
        workspace.phase = Phase::Closing;
        workspace.kill();
        if workspace.tabs.is_empty() {
            self.projects[p].remove_workspace(w);
        }
        self.defer_run_dirs(id, dirs);
    }
}
