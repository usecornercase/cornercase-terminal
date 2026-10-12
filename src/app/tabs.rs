use std::path::PathBuf;

use ratatui::layout::Rect;

use super::App;
use crate::error::Result;
use crate::project::{Project, Tab, Workspace};
use crate::split::Dir;
use crate::term::{SpawnOptions, Term};

impl App {
    pub(super) fn spawn(&mut self, area: Rect, cwd: PathBuf) -> Result<Term> {
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        let opts = SpawnOptions {
            id,
            shell: &self.shell,
            args: &[],
            env: &[],
            rows,
            cols,
            cwd: Some(cwd),
            theme: &self.theme,
        };
        Term::spawn(opts, self.tx.clone())
    }

    fn new_tab(&mut self, area: Rect, cwd: PathBuf, name: Option<String>) -> Result<Tab> {
        let term = self.spawn(area, cwd)?;
        Ok(Tab::new(self.take_id(), name, term))
    }

    pub(super) fn new_workspace(
        &mut self,
        area: Rect,
        path: PathBuf,
        name: Option<String>,
        worktree: bool,
    ) -> Result<Workspace> {
        let tab = self.new_tab(area, path.clone(), None)?;
        let mut workspace = Workspace::new(self.take_id(), path, name, worktree);
        workspace.tabs.push(tab);
        Ok(workspace)
    }

    pub(super) fn open_project(&mut self, dir: PathBuf, area: Rect) -> Result<()> {
        let path = dir.canonicalize().unwrap_or(dir);
        self.active = match self.projects.iter().position(|p| p.path == path) {
            Some(i) => i,
            None => self.add_project(path, area)?,
        };
        Ok(())
    }

    pub(super) fn add_project(&mut self, path: PathBuf, area: Rect) -> Result<usize> {
        let workspace = self.new_workspace(area, path.clone(), None, false)?;
        let mut project = Project::new(self.take_id(), path, None);
        project.workspaces.push(workspace);
        self.projects.push(project);
        self.sync_worktrees();
        Ok(self.projects.len() - 1)
    }

    pub(super) fn split_pane(&mut self, pane: u64, dir: Dir, area: Rect, focus: bool) -> Result<Option<u64>> {
        let Some((path, tab)) = self.tab_with_pane(pane) else { return Ok(None) };
        let cwd = tab
            .panes
            .iter()
            .find(|t| t.id == pane)
            .and_then(Term::cwd)
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| path.to_path_buf());
        let term = self.spawn(area, cwd)?;
        let id = term.id;
        if let Some((_, tab)) = self.tab_with_pane(pane) {
            tab.split(pane, dir, term, focus);
        }
        self.resize(area);
        Ok(Some(id))
    }

    pub(super) fn add_tab(&mut self, p: usize, w: usize, area: Rect) -> Result<()> {
        let t = self.push_tab(p, w, area, None)?;
        let project = &mut self.projects[p];
        project.workspaces[w].active = t;
        project.active = w;
        self.active = p;
        Ok(())
    }

    pub(super) fn push_tab(&mut self, p: usize, w: usize, area: Rect, name: Option<String>) -> Result<usize> {
        let path = self.projects[p].workspaces[w].path.clone();
        let tab = self.new_tab(area, path, name)?;
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        Ok(workspace.tabs.len() - 1)
    }

    pub(super) fn ensure_workspace(&mut self, p: usize) -> usize {
        if self.projects[p].workspaces.is_empty() {
            let id = self.take_id();
            let path = self.projects[p].path.clone();
            self.projects[p].workspaces.push(Workspace::new(id, path, None, false));
        }
        self.projects[p].active.min(self.projects[p].workspaces.len() - 1)
    }
}
