use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;

use super::App;
use super::event::Start;
use crate::accounts::Accounts;
use crate::control;
use crate::error::Result;
use crate::flows::folder;
use crate::log;
use crate::os::fs::canonical;
use crate::paths;
use crate::project::split::Dir;
use crate::project::{Hierarchy, Project, Tab, TabAt, Workspace, WorkspaceAt};
use crate::protocol;
use crate::term::{SpawnOptions, Term};

pub(super) struct Shells {
    default: String,
    pane_socket: Option<String>,
    editor_env: Vec<(String, String)>,
    refused: Option<(String, String)>,
}

pub(super) struct Chosen {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    own: bool,
}

struct NewPane {
    id: u64,
    size: (u16, u16),
    cwd: PathBuf,
    run: Option<PathBuf>,
}

impl Shells {
    pub(super) fn new(default: String) -> Self {
        Self { default, pane_socket: None, editor_env: Vec::new(), refused: None }
    }

    pub(super) fn default_shell(&self) -> &str {
        &self.default
    }

    pub(super) fn refused_for(&self, program: &str) -> Option<&str> {
        self.refused.as_ref().filter(|(was, _)| was == program).map(|(_, why)| why.as_str())
    }

    pub(super) fn pane_socket(&self) -> Option<&str> {
        self.pane_socket.as_deref()
    }

    pub(super) fn editor_env(&self) -> &[(String, String)] {
        &self.editor_env
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn set_editor(&mut self, visual: &str) {
        self.editor_env = vec![("VISUAL".into(), visual.into())];
    }
}

pub(super) struct Spawning<'a> {
    pub(super) id: u64,
    pub(super) size: (u16, u16),
    pub(super) shell: &'a str,
    pub(super) args: &'a [String],
    pub(super) env: &'a [(String, String)],
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
        let pane = NewPane { id, size, cwd, run: run.map(Path::to_path_buf) };
        self.spawn_pane(&pane, 0, accounts)
    }

    pub(super) fn chosen_shell(&mut self) -> Chosen {
        let setting = self.config.shell.clone();
        let Some(program) = setting.chosen() else {
            return Chosen { program: self.shells.default.clone(), args: setting.args, own: false };
        };
        match crate::term::shell::resolve(program, self.home.as_deref()) {
            Ok(path) => Chosen { program: path, args: setting.args, own: true },
            Err(why) => {
                self.shell_refused(program, &why);
                Chosen { program: self.shells.default.clone(), args: Vec::new(), own: false }
            }
        }
    }

    pub(super) fn shells_here(&self) -> crate::settings::ShellsHere {
        let program = self.config.shell.chosen();
        let problem = program.and_then(|program| {
            let refused = || self.shells.refused_for(program).map(str::to_string);
            crate::term::shell::resolve(program, self.home.as_deref()).err().or_else(refused)
        });
        crate::settings::ShellsHere {
            default: self.shells.default_shell().to_string(),
            found: crate::term::shell::found(),
            problem,
        }
    }

    fn shell_refused(&mut self, program: &str, why: &str) {
        let default = Path::new(&self.shells.default).file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let default = crate::process::program_name(default);
        let text = format!("cannot start the shell `{program}`: {why}; this pane runs {default}");
        self.messages.toast = Some(super::toast::Toast::new(text, crate::ui::ToastIcon::Bug));
        let key = (program.to_string(), why.to_string());
        if self.shells.refused.as_ref() != Some(&key) {
            log::warning!("app", "the shell cannot start, the default starts instead", program = program, why = why);
            self.shells.refused = Some(key);
            self.refresh_shells();
        }
    }

    fn spawn_pane(&mut self, pane: &NewPane, generation: u32, accounts: Accounts) -> Result<Term> {
        let chosen = self.chosen_shell();
        let first = self.spawn_chosen(pane, &chosen, generation, accounts.clone());
        let Err(crate::error::Error::SpawnShell { cause, .. }) = &first else {
            if first.is_ok() && chosen.own && self.shells.refused.take().is_some() {
                self.refresh_shells();
            }
            return first;
        };
        if !chosen.own && chosen.args.is_empty() {
            return first;
        }
        let why = cause.to_string();
        let program = self.config.shell.chosen().unwrap_or("the default shell with its arguments").to_string();
        self.shell_refused(&program, &why);
        let default = Chosen { program: self.shells.default.clone(), args: Vec::new(), own: false };
        self.spawn_chosen(pane, &default, generation, accounts)
    }

    fn spawn_chosen(&self, pane: &NewPane, chosen: &Chosen, generation: u32, accounts: Accounts) -> Result<Term> {
        let spawning = Spawning {
            id: pane.id,
            size: pane.size,
            shell: &chosen.program,
            args: &chosen.args,
            env: &[],
            cwd: pane.cwd.clone(),
            run: pane.run.as_deref(),
        };
        let mut term = self.spawn_as(spawning, generation, accounts)?;
        if !chosen.args.is_empty() {
            term.became_shell();
        }
        Ok(term)
    }

    pub(super) fn respawn_in(&mut self, pane: u64, cwd: PathBuf, accounts: Accounts) -> Result<bool> {
        let Some(old) = self.projects.pane_by(pane) else { return Ok(false) };
        let (generation, size, parent, collapsed) =
            (old.generation().wrapping_add(1), old.size(), old.parent.clone(), old.collapsed);
        let mut term = self.spawn_pane(&NewPane { id: pane, size, cwd, run: None }, generation, accounts)?;
        term.parent = parent;
        term.collapsed = collapsed;
        let Some(slot) = self.projects.pane_by_mut(pane) else { return Ok(false) };
        let old = std::mem::replace(slot, term);
        log::info!("app", "pane respawned", pane = pane, generation = generation);
        drop(old);
        Ok(true)
    }

    pub(super) fn spawn_term(&self, spawning: Spawning, accounts: Accounts) -> Result<Term> {
        self.spawn_as(spawning, 0, accounts)
    }

    fn spawn_as(&self, spawning: Spawning, generation: u32, accounts: Accounts) -> Result<Term> {
        let Spawning { id, size: (rows, cols), shell, args, env: extra, cwd, run } = spawning;
        accounts.link(|kind| self.accounts.default_home(kind));
        let mut env = self.pane_env(id, run, &accounts);
        env.extend_from_slice(extra);
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
        let mut term = Term::spawn_as(opts, generation, self.tx.clone())?;
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
        let path = canonical(dir);
        self.leave_remote();
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
        let accounts = source.map(|term| self.accounts.picked(&term.accounts().chosen())).unwrap_or_default();
        let term = self.spawn_in(area, cwd, None, accounts)?;
        let id = term.id;
        if let Some((_, tab)) = self.projects.tab_with_pane_mut(pane) {
            tab.split(pane, dir, term, focus);
        }
        if let Some(at) = self.projects.tab_of_pane(pane) {
            self.projects.workspace_mut(at.workspace()).stop_blaming_setup();
        }
        self.resize(area);
        Ok(Some(id))
    }

    pub(super) fn add_tab(&mut self, at: WorkspaceAt, area: Rect) -> Result<()> {
        let accounts = self.inherited_accounts(at);
        let tab = self.push_tab_in(at, area, None, accounts)?;
        self.show_tab(tab);
        Ok(())
    }

    fn inherited_accounts(&self, at: WorkspaceAt) -> Accounts {
        let pane = self.projects.active_tab(at).and_then(|tab| self.projects.tab(tab).pane()).map(|term| term.id);
        self.requested_accounts_from(pane, None, &BTreeMap::new()).unwrap_or_else(|_| self.active_accounts())
    }

    pub(super) fn show_tab(&mut self, at: TabAt) {
        self.leave_remote();
        self.projects.workspace_mut(at.workspace()).active = at.index();
        self.projects[at.project()].active = at.workspace().index();
        self.active = at.project();
    }

    pub(super) fn start_accounts(&self, start: Option<&Start>) -> Accounts {
        start.and_then(|start| start.accounts.clone()).unwrap_or_else(|| self.active_accounts())
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn push_tab(&mut self, at: WorkspaceAt, area: Rect, name: Option<String>) -> Result<TabAt> {
        let accounts = self.active_accounts();
        self.push_tab_in(at, area, name, accounts)
    }

    pub(super) fn push_tab_in(
        &mut self,
        at: WorkspaceAt,
        area: Rect,
        name: Option<String>,
        accounts: Accounts,
    ) -> Result<TabAt> {
        let workspace = self.projects.workspace_mut(at);
        workspace.stop_blaming_setup();
        let path = workspace.path.clone();
        let term = self.spawn_in(area, path, None, accounts)?;
        Ok(self.insert_tab(at, name, term))
    }

    pub(super) fn insert_tab(&mut self, at: WorkspaceAt, name: Option<String>, term: Term) -> TabAt {
        let tab = Tab::new(self.take_id(), name, term);
        self.projects.push_tab(at, tab)
    }

    pub(super) fn ensure_workspace(&mut self, p: usize) -> WorkspaceAt {
        let shown = self.projects[p].active.min(self.projects[p].workspaces.len().saturating_sub(1));
        if let Some(at) = self.projects.workspace_at(p, shown) {
            return at;
        }
        let id = self.take_id();
        let path = self.projects[p].path.clone();
        self.projects.push_workspace(p, Workspace::new(id, path, None, false))
    }
}
