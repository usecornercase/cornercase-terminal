use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::{Position, Rect};

use super::App;
use super::toast::Toast;
use crate::agents::AgentState;
use crate::agents::activity::Status;
use crate::agents::launch::Launch;
use crate::error::Result;
use crate::flows::{Role, folder};
use crate::log::{self};
use crate::project::{Hierarchy, Tab, Workspace};
use crate::state::{TabState, WorkspaceState};
use crate::term::Term;
use crate::ui;

pub(super) const RESUME_GRACE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct Parents {
    ids: HashMap<u64, u64>,
    wanted: Vec<(u64, Option<u64>, u64)>,
}

const SENDING: &str = "a message from `cornercase send` is going into this pane; wait a moment, so the two do not join";

impl App {
    pub(super) fn restore_workspace(
        &mut self,
        saved: &WorkspaceState,
        area: Rect,
        parents: &mut Parents,
    ) -> Option<(Workspace, usize)> {
        let path = saved.path.canonicalize().unwrap_or_else(|_| saved.path.clone());
        if !path.is_dir() {
            return None;
        }
        let id = self.take_id();
        let run_dir = self.restore_run_dir(saved);
        let mut tabs = Vec::new();
        let mut lost = 0;
        for (t, saved_tab) in saved.tabs.iter().enumerate().filter(|(_, t)| !t.panes.is_empty()) {
            let roles = |pane: usize| saved.run.as_ref().and_then(|run| run.role_at(t, pane));
            match self.restore_tab(saved_tab, &path, area, roles, run_dir.as_deref(), parents) {
                Ok(tab) => tabs.push((t, tab)),
                Err(e) => {
                    log::error!("server", "could not restore a tab", workspace = path.display(), error = e);
                    lost += 1;
                }
            }
        }
        if let Some(run) = &saved.run {
            self.restore_run(id, run, &mut tabs, run_dir.as_deref());
        }
        let tabs = tabs.into_iter().map(|(_, tab)| tab).collect();
        let mut workspace = saved.restore(id, path, tabs, &self.runs_dir);
        if let Some(dir) = run_dir.filter(|dir| !workspace.run_dirs.contains(dir)) {
            workspace.run_dirs.push(dir);
        }
        Some((workspace, lost))
    }

    fn restore_run_dir(&self, saved: &WorkspaceState) -> Option<PathBuf> {
        let name = saved.run.as_ref()?.dir.as_deref().filter(|name| folder::valid(name))?;
        let dir = self.runs_dir.join(name);
        match folder::ensure(&dir) {
            Ok(true) => log::info!("flow", "run folder made again", path = dir.display()),
            Ok(false) => {}
            Err(e) => {
                log::warning!("flow", "cannot use the run's folder", path = dir.display(), error = e);
                return None;
            }
        }
        Some(dir)
    }

    pub(super) fn restore_tab<'a>(
        &mut self,
        saved: &TabState,
        dir: &Path,
        area: Rect,
        roles: impl Fn(usize) -> Option<&'a Role>,
        run: Option<&Path>,
        parents: &mut Parents,
    ) -> Result<Tab> {
        let mut panes = Vec::new();
        for (i, pane) in saved.panes.iter().enumerate() {
            let cwd = pane.cwd.clone().filter(|cwd| cwd.is_dir()).unwrap_or_else(|| dir.to_path_buf());
            let role = roles(i);
            let apart = role.is_some_and(Role::own_worktree) && pane.cwd.as_ref().is_none_or(|cwd| !cwd.is_dir());
            let run = role.and(run);
            let mut term = self.spawn_with(area, cwd, run)?;
            if apart {
                log::warning!("server", "a role's worktree is gone, so its agent does not resume", pane = term.id);
                let name = role.map(|role| role.name.as_str()).unwrap_or_default();
                let message = format!("the worktree of {name} is gone: it works in the run's worktree now");
                self.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
            } else if self.resume(&mut term, pane.agent.as_ref(), role, run) {
                term.agent.restore(pane.done, pane.pending);
            }
            if let Some(old) = pane.id {
                parents.ids.insert(old, term.id);
            }
            if let Some(parent) = pane.parent {
                parents.wanted.push((term.id, pane.id, parent));
            }
            panes.push(term);
        }
        Ok(saved.restore(self.take_id(), panes))
    }

    pub(super) fn adopt_saved(&mut self, parents: Parents) {
        for (child, old, parent) in parents.wanted {
            let found = parents.ids.get(&parent).copied().filter(|_| old.is_none_or(|old| parent < old));
            let found = found.filter(|&new| new != child && self.projects.pane_by(new).is_some());
            match (found, self.projects.pane_by_mut(child)) {
                (Some(new), Some(term)) => term.parent = Some(new),
                _ => log::debug!("server", "a saved parent was dropped", pane = child, parent = parent),
            }
        }
    }

    fn resume(&mut self, term: &mut Term, agent: Option<&AgentState>, role: Option<&Role>, run: Option<&Path>) -> bool {
        let Some(agent) = agent.filter(|_| self.config.resume_agents) else { return false };
        let Some(line) = self.resume_line(role, agent, run) else { return false };
        log::info!("app", "resuming a conversation", pane = term.id, agent = agent.kind);
        let now = Instant::now();
        let trusted = role.is_some_and(|role| role.agent == agent.kind);
        self.agents.launches.push(Launch::resume(term.id, line, trusted, now, now + RESUME_GRACE));
        term.resume = Some(agent.clone());
        term.resuming = Some(now + RESUME_GRACE);
        true
    }

    pub(super) fn resume_holds(&self, pane: u64) -> bool {
        let now = Instant::now();
        self.agents.launches.iter().any(|launch| launch.term == pane && launch.holds(now))
    }

    pub(super) fn pane_bands(&self) -> HashMap<u64, ui::PaneBand> {
        let now = Instant::now();
        let mut bands: HashMap<u64, ui::PaneBand> = self
            .held_labels()
            .into_iter()
            .map(|(pane, (text, queued))| (pane, ui::PaneBand::Held { text, queued }))
            .collect();
        for launch in self.agents.launches.iter().filter(|launch| launch.holds(now)) {
            if let Some(term) = self.projects.pane_by(launch.term) {
                bands.insert(term.id, ui::PaneBand::Resuming(resumed(term)));
            }
        }
        bands
    }

    pub(super) fn holds_input(&mut self) -> bool {
        let Some(term) = self.term() else { return false };
        let message = if self.resume_holds(term.id) {
            format!("{} is resuming its conversation in this pane; wait for it, or press cancel", resumed(term))
        } else if self.sending(term.id) {
            SENDING.to_string()
        } else {
            return false;
        };
        self.toast = Some(Toast::new(message, ui::ToastIcon::Agent(Status::Working)));
        true
    }

    pub(super) fn press_held_notice(&mut self, area: Rect, pos: Position) -> bool {
        let Some(tab) = self.tab() else { return false };
        let pressed = tab.shown(area).into_iter().find_map(|(id, rect)| {
            let (_, queued) = self.held_label(id).filter(|_| !self.resume_holds(id))?;
            let bar = ui::held_notice(rect, tab.pane_by(id)?.emulator.below(), queued)?;
            match (bar.send.contains(pos), bar.wait.is_some_and(|wait| wait.contains(pos))) {
                (true, _) => Some((id, true)),
                (_, true) => Some((id, false)),
                _ => None,
            }
        });
        match pressed {
            Some((pane, true)) => self.send_held_now(pane),
            Some((pane, false)) => self.queue_held(pane),
            None => return false,
        }
        true
    }

    pub(super) fn press_resume_notice(&mut self, area: Rect, pos: Position) -> bool {
        let Some(tab) = self.tab() else { return false };
        let pressed = tab.shown(area).into_iter().find_map(|(id, rect)| {
            let term = tab.pane_by(id)?;
            ui::resume_notice(rect, &resumed(term))
                .filter(|(_, button)| self.resume_holds(id) && button.contains(pos))
                .map(|_| id)
        });
        let Some(pane) = pressed else { return false };
        self.cancel_resume(pane);
        true
    }

    fn cancel_resume(&mut self, pane: u64) {
        let Some(i) = self.agents.launches.iter().position(|launch| launch.term == pane && launch.resume.is_some())
        else {
            return;
        };
        let typed = self.agents.launches.remove(i).typed();
        if !typed && let Some(term) = self.projects.pane_by_mut(pane) {
            term.resuming = None;
            term.resume = None;
            term.agent.give_up();
        }
        log::info!("app", "resume cancelled", pane = pane, typed = typed);
    }
}

pub(super) fn resumed(term: &Term) -> String {
    term.resume.as_ref().map_or_else(|| "the agent".to_string(), |agent| agent.kind.clone())
}

pub(super) fn not_restored(missed: &[(String, usize)]) -> String {
    let tabs = match missed.iter().map(|(_, n)| n).sum::<usize>() {
        1 => "1 tab".to_string(),
        n => format!("{n} tabs"),
    };
    let projects: Vec<&str> = missed.iter().map(|(name, _)| name.as_str()).collect();
    format!("could not restore {tabs} ({}), see server.log", projects.join(", "))
}
