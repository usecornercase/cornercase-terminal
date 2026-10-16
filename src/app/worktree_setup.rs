use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use super::App;
use super::toast::Toast;
use crate::agents::activity::Status;
use crate::error::Result;
use crate::git;
use crate::git::worktree::setup;
use crate::host::notify::Notification;
use crate::log;
use crate::project::{Hierarchy, Tab};
use crate::term::{SpawnOptions, Term};
use crate::ui;

impl App {
    pub(crate) fn set_setup_dir(&mut self, dir: &Path) {
        self.setup_dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    }

    pub(super) fn setup_script(&self, p: usize) -> Option<String> {
        let text = self.config.project(&self.projects[p].path, self.home.as_deref()).setup;
        (!text.trim().is_empty()).then_some(text)
    }

    pub(super) fn setup_note(&self, project: u64, worktree: Option<bool>) -> Option<ui::Note> {
        let script = self.setup_script(self.projects.project_index(project)?).filter(|_| worktree == Some(true))?;
        let first = script.lines().map(str::trim).find(|line| !line.is_empty())?;
        let more = if script.lines().filter(|line| !line.trim().is_empty()).count() > 1 { " …" } else { "" };
        Some(ui::Note::Info(format!("then runs the project's setup: {first}{more}")))
    }

    pub(super) fn teardown_script(&self, p: usize) -> Option<String> {
        let text = self.config.project(&self.projects[p].path, self.home.as_deref()).teardown;
        (!text.trim().is_empty()).then_some(text)
    }

    pub(super) fn spawn_setup(&mut self, area: Rect, p: usize, path: PathBuf, script: &str) -> Result<Term> {
        let (rows, cols) = self.pane_size(area);
        let id = self.take_id();
        if let Err(e) = std::fs::create_dir_all(&self.setup_dir) {
            log::warning!("worktree", "cannot make the setup folder", error = e);
        }
        let status = setup::status_file(&self.setup_dir, id);
        let _ = std::fs::remove_file(&status);
        let branch = git::branch(&path).unwrap_or_default();
        let args = setup::args(script, &status, &self.shell, &self.projects[p].path, &branch);
        let env = Self::pane_env(id);
        let opts = SpawnOptions {
            id,
            shell: "/bin/sh",
            args: &args,
            env: &env,
            rows,
            cols,
            cwd: Some(path),
            theme: &self.theme,
        };
        let mut term = Term::spawn(opts, self.tx.clone())?;
        term.setup = Some(setup::Pane::Running { status, since: Instant::now(), seen: false });
        log::info!("worktree", "setup started", pane = id, bytes = script.len());
        Ok(term)
    }

    pub(super) fn push_setup_tab(&mut self, p: usize, w: usize, area: Rect, name: Option<String>) -> Result<usize> {
        let Some(script) = self.setup_script(p) else { return self.push_tab(p, w, area, name) };
        let path = self.projects[p].workspaces[w].path.clone();
        let term = self.spawn_setup(area, p, path, &script)?;
        let tab = Tab::new(self.take_id(), name, term);
        let workspace = &mut self.projects[p].workspaces[w];
        workspace.tabs.push(tab);
        Ok(workspace.tabs.len() - 1)
    }

    pub(super) fn setup_action(&self, project: u64, workspace: u64) -> Option<super::menus::MenuAction> {
        let (p, w) = self.projects.workspace_in(project, workspace)?;
        let target = &self.projects[p].workspaces[w];
        let offered = target.worktree && target.open() && !target.setting_up() && self.setup_script(p).is_some();
        offered.then_some(super::menus::MenuAction::RunSetup(workspace))
    }

    pub(super) fn run_setup(&mut self, workspace: u64, area: Rect) {
        let Some((p, w)) = self.projects.workspace_index(workspace) else { return };
        if self.setup_script(p).is_none() || self.projects[p].workspaces[w].setting_up() {
            return;
        }
        match self.push_setup_tab(p, w, area, None) {
            Ok(t) => {
                self.projects[p].workspaces[w].active = t;
                self.projects[p].active = w;
                self.active = p;
            }
            Err(e) => self.toast = Some(Toast::new(e.to_string(), ui::ToastIcon::Bug)),
        }
    }

    pub(super) fn held_launch_panes(&self) -> Vec<u64> {
        let held = self.projects.iter().flat_map(|p| &p.workspaces).filter(|w| w.setting_up());
        held.flat_map(|w| w.tabs.iter().flat_map(|t| t.panes.iter().map(|term| term.id))).collect()
    }

    pub(super) fn setup_failure(&self, pane: u64) -> Option<String> {
        let workspace = self
            .projects
            .iter()
            .flat_map(|p| &p.workspaces)
            .find(|w| w.tabs.iter().any(|t| t.index_of(pane).is_some()))?;
        let (at, state) = workspace.setup()?;
        let setup::Pane::Failed { code, .. } = state else { return None };
        Some(format!(
            "the setup of workspace {} failed with {code}, so its agent did not start; \
             `cornercase read --pane {at}` shows its output",
            workspace.id
        ))
    }

    pub(super) fn watch_setups(&mut self, now: Instant) {
        let visible = self.visible_tab();
        let mut ended = Vec::new();
        for project in &mut self.projects {
            for tab in project.workspaces.iter_mut().flat_map(|w| &mut w.tabs) {
                let shown = visible == Some(tab.id);
                for term in &mut tab.panes {
                    let shell = term.shell_pid();
                    match &mut term.setup {
                        Some(setup::Pane::Running { status, seen, .. }) => {
                            let running = shell.is_some_and(setup::running_args);
                            *seen |= running;
                            let gone = *seen && shell.is_some() && !running;
                            if let Some(code) = setup::read_status(status) {
                                ended.push((term.id, Some(code)));
                            } else if gone {
                                ended.push((term.id, None));
                            }
                        }
                        Some(setup::Pane::Failed { unseen, .. }) if shown => *unseen = false,
                        _ => {}
                    }
                }
            }
        }
        for (pane, code) in ended {
            self.setup_finished(pane, code, now);
        }
    }

    fn setup_finished(&mut self, pane: u64, code: Option<i32>, now: Instant) {
        let visible = self.visible_tab();
        let Some((project, workspace, tab)) = self.projects.iter().find_map(|p| {
            p.workspaces
                .iter()
                .find_map(|w| w.tabs.iter().find(|t| t.index_of(pane).is_some()).map(|t| (p.id, w.id, t.id)))
        }) else {
            return;
        };
        let Some(term) = self.projects.pane_by_mut(pane) else { return };
        let Some(setup::Pane::Running { status, since, .. }) = term.setup.take() else { return };
        term.became_shell();
        let _ = std::fs::remove_file(&status);
        let ms = u64::try_from(now.saturating_duration_since(since).as_millis()).unwrap_or(u64::MAX);
        let panes = self.workspace_panes(workspace);
        if code == Some(0) {
            log::info!("worktree", "setup finished", pane = pane, status = 0, ms = ms);
            for launch in self.agents.launches.iter_mut().filter(|l| panes.contains(&l.term)) {
                launch.wait_from(now);
            }
            return;
        }
        let failed = code.map_or_else(|| "no exit status".to_string(), |c| format!("exit status {c}"));
        log::warning!("worktree", "setup failed", pane = pane, status = failed, ms = ms);
        if let Some(term) = self.projects.pane_by_mut(pane) {
            term.setup = Some(setup::Pane::Failed { code: failed, unseen: visible != Some(tab) });
        }
        let dropped: Vec<_> = self.agents.launches.iter().filter(|l| panes.contains(&l.term)).map(|l| l.key).collect();
        self.agents.launches.retain(|l| !panes.contains(&l.term));
        for key in dropped.iter().flatten() {
            self.requests.launched(*key, false);
        }
        let ended = self.runs.iter().filter(|r| r.workspace == workspace).count();
        self.runs.retain(|r| r.workspace != workspace);
        if ended > 0 {
            log::info!("flow", "run ended", workspace = workspace, reason = "setup failed");
        }
        let Some((p, w)) = self.projects.workspace_in(project, workspace) else { return };
        let place = format!("{} › {}", self.project_label(&self.projects[p]), self.projects[p].workspaces[w].label());
        let tail = if dropped.is_empty() && ended == 0 { "" } else { ": its agent did not start" };
        let message = format!("setup failed in {place}{tail}");
        if visible != Some(tab) {
            self.outbox
                .notifications
                .extend(Notification::new(&crate::host::notify::clean(&message), &self.config.desktop_notifications));
        }
        self.toast = Some(Toast::new(message, ui::ToastIcon::Agent(Status::Failed)));
    }

    fn workspace_panes(&self, workspace: u64) -> Vec<u64> {
        let found = self.projects.iter().flat_map(|p| &p.workspaces).find(|w| w.id == workspace);
        found.into_iter().flat_map(|w| w.tabs.iter().flat_map(|t| t.panes.iter().map(|term| term.id))).collect()
    }
}
