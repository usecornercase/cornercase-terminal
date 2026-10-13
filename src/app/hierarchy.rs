use std::path::Path;

use super::App;
use super::menus::Target;
use super::overlay::Overlay;
use crate::log::{self};
use crate::project::{Group, Phase, Project, Tab, Workspace, shift_active};
use crate::search::Goto;
use crate::term::Term;
use crate::ui::{self};

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

    pub(super) fn tab_with_pane(&mut self, pane: u64) -> Option<(&Path, &mut Tab)> {
        self.projects.iter_mut().flat_map(|p| &mut p.workspaces).find_map(|w| {
            let tab = w.tabs.iter_mut().find(|t| t.panes.iter().any(|term| term.id == pane))?;
            Some((w.path.as_path(), tab))
        })
    }

    pub(super) fn project_index(&self, id: u64) -> Option<usize> {
        self.projects.iter().position(|p| p.id == id)
    }

    pub(super) fn workspace_index(&self, project: u64, workspace: u64) -> Option<(usize, usize)> {
        let p = self.project_index(project)?;
        let w = self.projects[p].workspaces.iter().position(|w| w.id == workspace)?;
        Some((p, w))
    }

    pub(super) fn group_index(&self, id: u64) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    pub(super) fn group(&self, id: u64) -> Option<&ui::GroupEntry> {
        self.groups.iter().find(|g| g.id == id).map(|g| &g.entry)
    }

    pub(super) fn group_mut(&mut self, id: u64) -> Option<&mut ui::GroupEntry> {
        self.groups.iter_mut().find(|g| g.id == id).map(|g| &mut g.entry)
    }

    pub(super) fn project_groups(&self) -> Vec<Option<usize>> {
        self.projects.iter().map(|p| p.group.and_then(|id| self.group_index(id))).collect()
    }

    pub(super) fn project_label(&self, project: &Project) -> String {
        project.name.clone().unwrap_or_else(|| crate::text::folder_name(&project.path, self.home.as_deref()))
    }

    pub(super) fn remove(&mut self, id: u64) {
        if let Some(term) = self.projects.iter_mut().flat_map(Project::terms_mut).find(|t| t.id == id) {
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
            Some(Overlay::CloseProject { project }) => self.project_index(project).is_none(),
            Some(Overlay::CloseWorkspace { project, workspace }) => self.workspace_index(project, workspace).is_none(),
            Some(Overlay::CloseTab { project, workspace, tab }) => self.tab_index(project, workspace, tab).is_none(),
            _ => false,
        }
    }

    pub(super) fn reap(&mut self) {
        let exited: Vec<u64> =
            self.projects.iter_mut().flat_map(Project::terms_mut).filter_map(|t| t.exited().then_some(t.id)).collect();
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
        let Some(p) = self.project_index(id) else { return };
        self.forget_project_changes(p);
        let project = &mut self.projects[p];
        project.closing = true;
        project.kill();
        if !project.has_terms() {
            self.remove_project(p);
        }
    }

    pub(super) fn toggle_fold(&mut self, target: Target) {
        match target {
            Target::Project(id) => {
                if let Some(p) = self.project_index(id) {
                    self.projects[p].collapsed = !self.projects[p].collapsed;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    let workspace = &mut self.projects[p].workspaces[w];
                    workspace.collapsed = !workspace.collapsed;
                }
            }
            Target::Group(_) | Target::Tab(..) => self.click_row(target),
        }
    }

    pub(super) fn close_tab(&mut self, p: usize, w: usize, t: usize) {
        for term in &mut self.projects[p].workspaces[w].tabs[t].panes {
            term.kill();
        }
    }

    pub(super) fn move_project(&mut self, id: u64, group: Option<usize>, before: Option<usize>) {
        let group = group.and_then(|g| self.groups.get(g)).map(|g| g.id);
        let before = before.and_then(|q| self.projects.get(q)).map(|q| q.id);
        let active = self.project().map(|p| p.id);
        let Some(p) = self.project_index(id) else { return };
        let mut project = self.projects.remove(p);
        project.group = group;
        self.cancel_pending_group_deletion(group, project.closing);
        let at = before.and_then(|q| self.project_index(q)).unwrap_or_else(|| {
            self.projects.iter().rposition(|p| p.group == group).map_or(self.projects.len(), |i| i + 1)
        });
        self.projects.insert(at, project);
        self.active = active.and_then(|id| self.project_index(id)).unwrap_or(0);
    }

    pub(super) fn tab_index(&self, project: u64, workspace: u64, tab: u64) -> Option<(usize, usize, usize)> {
        let (p, w) = self.workspace_index(project, workspace)?;
        let t = self.projects[p].workspaces[w].tabs.iter().position(|t| t.id == tab)?;
        Some((p, w, t))
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
        let Some(p) = self.project_index(project) else { return };
        self.active = p;
        let project = &mut self.projects[p];
        if let Some(w) = workspace.and_then(|id| project.workspaces.iter().position(|w| w.id == id)) {
            project.active = w;
            let workspace = &mut project.workspaces[w];
            if let Some(t) = tab.and_then(|id| workspace.tabs.iter().position(|t| t.id == id)) {
                workspace.active = t;
            }
        }
        self.unfold_focus();
    }

    pub(super) fn current_name(&self, target: Target) -> Option<String> {
        match target {
            Target::Group(id) => self.group(id).map(|g| g.name.clone()),
            Target::Project(id) => self.project_index(id).map(|p| self.project_label(&self.projects[p])),
            Target::Workspace(project, workspace) => {
                self.workspace_index(project, workspace).map(|(p, w)| self.projects[p].workspaces[w].label())
            }
            Target::Tab(project, workspace, tab) => {
                let (p, w) = self.workspace_index(project, workspace)?;
                self.projects[p].workspaces[w].tabs.iter().find(|t| t.id == tab).map(|t| t.label(&self.config))
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
                if let Some(p) = self.project_index(id) {
                    self.projects[p].name = name;
                }
            }
            Target::Workspace(project, workspace) => {
                if let Some((p, w)) = self.workspace_index(project, workspace) {
                    self.projects[p].workspaces[w].name = name;
                }
            }
            Target::Tab(project, workspace, tab) => {
                if let Some((p, w)) = self.workspace_index(project, workspace)
                    && let Some(t) = self.projects[p].workspaces[w].tabs.iter_mut().find(|t| t.id == tab)
                {
                    t.name = name;
                }
            }
        }
    }

    pub(super) fn delete_group(&mut self, id: u64) {
        self.groups.retain(|g| g.id != id);
        self.deleting_groups.retain(|&g| g != id);
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
        let taken =
            |&(icon, colour): &(char, u8)| self.groups.iter().any(|g| (g.entry.icon, g.entry.colour) == (icon, colour));
        let (icon, colour) = ui::GROUP_STYLES
            .iter()
            .copied()
            .find(|style| !taken(style))
            .unwrap_or(ui::GROUP_STYLES[self.groups.len() % ui::GROUP_STYLES.len()]);
        let id = self.take_id();
        self.groups.push(Group { id, entry: ui::GroupEntry { name, icon, colour, collapsed: false } });
        id
    }

    pub(super) fn removing(&self, p: usize, w: usize) -> bool {
        self.projects.get(p).and_then(|p| p.workspaces.get(w)).is_some_and(Workspace::removing)
    }

    pub(super) fn drop_workspace(&mut self, project: u64, workspace: u64) {
        let Some((p, w)) = self.workspace_index(project, workspace) else { return };
        self.changes.forget(workspace);
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.phase = Phase::Closing;
        workspace.kill();
        if workspace.tabs.is_empty() {
            self.projects[p].remove_workspace(w);
        }
    }

    pub(super) fn workspace_position(&self, id: u64) -> Option<(usize, usize)> {
        self.projects
            .iter()
            .enumerate()
            .find_map(|(p, project)| project.workspaces.iter().position(|w| w.id == id).map(|w| (p, w)))
    }
}
