use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use super::App;
use super::event::Start;
use crate::accounts::Accounts;
use crate::control;
use crate::error::Result;
use crate::flows::folder;
use crate::paths;
use crate::project::split::Dir;
use crate::project::{Hierarchy, Project, Tab, Workspace};
use crate::protocol;
use crate::term::{SpawnOptions, Term};

pub(super) struct Shells {
    shell: String,
    pane_socket: Option<String>,
    editor_env: Vec<(String, String)>,
}

impl Shells {
    pub(super) fn new(shell: String) -> Self {
        Self { shell, pane_socket: None, editor_env: Vec::new() }
    }

    pub(super) fn shell(&self) -> &str {
        &self.shell
    }

    pub(super) fn editor_env(&self) -> &[(String, String)] {
        &self.editor_env
    }

    #[cfg(test)]
    pub(super) fn set_editor(&mut self, visual: &str) {
        self.editor_env = vec![("VISUAL".into(), visual.into())];
    }
}

pub(super) struct Spawning<'a> {
    pub(super) id: u64,
    pub(super) size: (u16, u16),
    pub(super) shell: &'a str,
    pub(super) args: &'a [String],
    pub(super) cwd: PathBuf,
    pub(super) run: Option<&'a Path>,
}

impl App {
    pub(crate) fn set_pane_socket(&mut self, socket: &Path) {
        self.shells.pane_socket = socket.to_str().map(str::to_owned);
    }

    pub(crate) fn set_accounts_dir(&mut self, dir: &Path) {
        self.accounts.set_dir(dir);
    }

    #[cfg(test)]
    pub(crate) fn set_default_homes(&mut self, dir: &Path) {
        self.accounts.set_default_homes(dir);
    }

    pub(super) fn active_accounts(&self) -> Accounts {
        self.accounts.picked(self.config.selected_accounts())
    }

    pub(super) fn pane_env(&self, id: u64, run: Option<&Path>, accounts: &Accounts) -> Vec<(String, String)> {
        let mut env = vec![
            (protocol::NESTED_ENV.to_string(), "1".to_string()),
            (control::PANE_ENV.to_string(), id.to_string()),
            (control::SERVER_ENV.to_string(), control::server_token().to_string()),
        ];
        if let Some(dir) = run.and_then(Path::to_str) {
            env.push((folder::ENV.to_string(), dir.to_string()));
        }
        if let Some(socket) = &self.shells.pane_socket {
            env.push((paths::SOCKET_ENV.to_string(), socket.clone()));
        }
        env.extend(accounts.env());
        env
    }

    pub(super) fn spawn(&mut self, area: Rect, cwd: PathBuf) -> Result<Term> {
        let accounts = self.active_accounts();
        self.spawn_in(area, cwd, None, accounts)
    }

    pub(super) fn spawn_in(
        &mut self,
        area: Rect,
        cwd: PathBuf,
        run: Option<&Path>,
        accounts: Accounts,
    ) -> Result<Term> {
        let size = self.pane_size(area);
        let id = self.take_id();
        self.spawn_term(Spawning { id, size, shell: &self.shells.shell, args: &[], cwd, run }, accounts)
    }

    pub(super) fn spawn_term(&self, spawning: Spawning, accounts: Accounts) -> Result<Term> {
        let Spawning { id, size: (rows, cols), shell, args, cwd, run } = spawning;
        accounts.link(|kind| self.accounts.default_home(kind));
        let env = self.pane_env(id, run, &accounts);
        let removed = accounts.removed();
        let opts = SpawnOptions {
            id,
            shell,
            args,
            env: &env,
            remove: &removed,
            rows,
            cols,
            cwd: Some(cwd),
            theme: self.terminal_theme(),
        };
        let mut term = Term::spawn(opts, self.tx.clone())?;
        term.set_accounts(accounts);
        Ok(term)
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
        let Some((path, tab)) = self.projects.tab_with_pane(pane) else { return Ok(None) };
        let source = tab.pane_by(pane);
        let cwd = source.and_then(Term::cwd).filter(|dir| dir.is_dir()).unwrap_or_else(|| path.to_path_buf());
        let accounts =
            source.map(|term| self.accounts.picked(&term.accounts().chosen())).unwrap_or_default();
        let term = self.spawn_in(area, cwd, None, accounts)?;
        let id = term.id;
        if let Some((_, tab)) = self.projects.tab_with_pane_mut(pane) {
            tab.split(pane, dir, term, focus);
        }
        for workspace in self.projects.iter_mut().flat_map(|p| &mut p.workspaces) {
            if workspace.tabs.iter().any(|t| t.index_of(pane).is_some()) {
                workspace.stop_blaming_setup();
            }
        }
        self.resize(area);
        Ok(Some(id))
    }

    pub(super) fn add_tab(&mut self, p: usize, w: usize, area: Rect) -> Result<()> {
        let t = self.push_tab(p, w, area, None)?;
        self.show_tab(p, w, t);
        Ok(())
    }

    pub(super) fn show_tab(&mut self, p: usize, w: usize, t: usize) {
        let project = &mut self.projects[p];
        project.workspaces[w].active = t;
        project.active = w;
        self.active = p;
    }

    pub(super) fn start_accounts(&self, start: Option<&Start>) -> Accounts {
        start.and_then(|start| start.accounts.clone()).unwrap_or_else(|| self.active_accounts())
    }

    pub(super) fn push_tab(&mut self, p: usize, w: usize, area: Rect, name: Option<String>) -> Result<usize> {
        let accounts = self.active_accounts();
        self.push_tab_in((p, w), area, name, accounts)
    }

    pub(super) fn push_tab_in(
        &mut self,
        place: (usize, usize),
        area: Rect,
        name: Option<String>,
        accounts: Accounts,
    ) -> Result<usize> {
        let (p, w) = place;
        self.projects[p].workspaces[w].stop_blaming_setup();
        let path = self.projects[p].workspaces[w].path.clone();
        let term = self.spawn_in(area, path, None, accounts)?;
        Ok(self.insert_tab(place, name, term))
    }

    pub(super) fn insert_tab(&mut self, (p, w): (usize, usize), name: Option<String>, term: Term) -> usize {
        let tab = Tab::new(self.take_id(), name, term);
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        workspace.tabs.len() - 1
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
