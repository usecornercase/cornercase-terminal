use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::App;
use super::event::AppEvent;
use super::toast::Toast;
use super::worktrees::RemoveOptions;
use crate::agents::activity::Status;
use crate::git;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::queue::State;
use crate::{control, ui, worktrees};

pub(crate) const CLEANUP_GRACE: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Copy)]
struct Countdown {
    since: Instant,
    done: Option<Instant>,
    checking: bool,
}

pub(super) struct Cleaning {
    run: u64,
    repo: PathBuf,
    base: Option<String>,
    branches: Vec<String>,
}

#[derive(Default)]
pub(super) struct Cleanups {
    countdowns: HashMap<u64, Countdown>,
    cleaning: HashMap<u64, Cleaning>,
}

impl Cleanups {
    pub(super) fn arm(&mut self, id: u64, now: Instant) {
        self.countdowns.insert(id, Countdown { since: now, done: None, checking: false });
    }

    pub(super) fn take_cleaning(&mut self, workspace: u64) -> Option<Cleaning> {
        self.cleaning.remove(&workspace)
    }
}

#[derive(Debug)]
pub(crate) enum RoutineEvent {
    Checked { run: u64, workspace: u64, kept: Option<String> },
}

impl App {
    pub(super) fn drive_cleanups(&mut self, now: Instant) {
        let ids: Vec<u64> = self.panels.queued.cleanups.countdowns.keys().copied().collect();
        for id in ids {
            self.drive_cleanup(id, now);
        }
    }

    fn drive_cleanup(&mut self, id: u64, now: Instant) {
        let Some(countdown) = self.panels.queued.cleanups.countdowns.get(&id).copied() else { return };
        if countdown.checking {
            return;
        }
        let finished = self.run_finished(id, countdown.since);
        let cleanups = &mut self.panels.queued.cleanups;
        let Some(finished) = finished else {
            cleanups.countdowns.remove(&id);
            return;
        };
        let done = finished.then(|| countdown.done.unwrap_or(now));
        let ripe = done.is_some_and(|done| now.saturating_duration_since(done) >= CLEANUP_GRACE);
        if let Some(entry) = cleanups.countdowns.get_mut(&id) {
            entry.done = done;
            entry.checking = ripe;
        }
        if ripe && let Some(workspace) = self.run_workspace(id) {
            self.check_cleanup(id, workspace);
        }
    }

    fn run_workspace(&self, id: u64) -> Option<u64> {
        self.panels.queued.queue().entry(id)?.task.place.workspace_id
    }

    fn run_finished(&self, id: u64, since: Instant) -> Option<bool> {
        let entry = self.panels.queued.queue().entry(id)?;
        let pane = match &entry.state {
            State::Starting => return Some(false),
            State::Started { pane, server } if server == control::server_token() => *pane,
            _ => return None,
        };
        let workspace = entry.task.place.workspace_id?;
        if !self.projects.workspace_by(workspace)?.open() {
            return None;
        }
        if entry.task.flow.is_some() {
            return Some(self.flow_finished(workspace));
        }
        let term = self.projects.pane_by(pane)?;
        Some(term.agent.status() == Some(Status::Done) && term.agent.worked_since(since))
    }

    fn flow_finished(&self, workspace: u64) -> bool {
        let Some(run) = self.runs.of_workspace(workspace) else { return false };
        let busy = |pane: u64| {
            let status = self.projects.pane_by(pane).and_then(|term| term.agent.status());
            matches!(status, Some(Status::Working | Status::Waiting))
        };
        run.reached(self.pull_request(workspace)) && !run.members.iter().any(|member| busy(member.pane))
    }

    fn check_cleanup(&mut self, run: u64, workspace: u64) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        let found = self.projects.workspace(at);
        let paths: Vec<PathBuf> = std::iter::once(found.path.clone())
            .chain(found.role_worktrees.iter().map(|tree| tree.path.clone()))
            .collect();
        let base = crate::changes::git::default_base(&self.projects[at.project()].path);
        let job = Job::new(Level::Info, "routine", "check the workspace").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(worktrees::scan::left_over(&paths, base.as_deref())),
            move |result, _| {
                let kept = result.unwrap_or_else(|error| Some(format!("could not check it: {error}")));
                Some(AppEvent::Routine(Box::new(RoutineEvent::Checked { run, workspace, kept })))
            },
        );
    }

    pub(super) fn routine_event(&mut self, event: RoutineEvent) {
        let RoutineEvent::Checked { run, workspace, kept } = event;
        let Some(countdown) = self.panels.queued.cleanups.countdowns.get_mut(&run) else { return };
        countdown.checking = false;
        let since = countdown.since;
        if let Some(reason) = kept {
            self.keep_run(run, &reason);
        } else if self.run_finished(run, since) == Some(true) {
            self.remove_run_workspace(run, workspace);
        } else if let Some(countdown) = self.panels.queued.cleanups.countdowns.get_mut(&run) {
            countdown.done = None;
        }
    }

    fn remove_run_workspace(&mut self, run: u64, workspace: u64) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        let project = self.projects[at.project()].id;
        let repo = self.projects[at.project()].path.clone();
        let found = self.projects.workspace(at);
        let mut branches: Vec<String> = found.role_worktrees.iter().map(|tree| tree.branch.clone()).collect();
        branches.extend(git::branch(&found.path));
        let base = crate::changes::git::default_base(&repo);
        if self.start_removal(project, workspace, RemoveOptions::default(), None) {
            self.panels.queued.cleanups.cleaning.insert(workspace, Cleaning { run, repo, base, branches });
        } else {
            self.keep_run(run, "it could not be stopped");
        }
    }

    pub(super) fn run_removed(
        &mut self,
        project: u64,
        workspace: u64,
        cleaning: Cleaning,
        result: crate::error::Result<Option<String>>,
    ) {
        let Cleaning { run, repo, base, branches } = cleaning;
        self.settle_removal(project, workspace, result.is_ok());
        self.panels.queued.cleanups.countdowns.remove(&run);
        match result {
            Ok(_) => {
                self.panels.queued.set_state(run, State::Removed);
                self.delete_run_branches(repo, base, branches);
            }
            Err(error) => self.keep_run(run, &format!("could not remove it: {error}")),
        }
        self.sync_queued();
    }

    fn delete_run_branches(&self, repo: PathBuf, base: Option<String>, branches: Vec<String>) {
        let job = Job::new(Level::Info, "routine", "delete branches").with("branches", branches.join(","));
        self.spawn_job(
            job,
            move || {
                for branch in &branches {
                    delete_clean_branch(&repo, base.as_deref(), branch);
                }
                Ok(())
            },
            |_, _| None,
        );
    }

    fn keep_run(&mut self, run: u64, reason: &str) {
        self.panels.queued.cleanups.countdowns.remove(&run);
        let Some(entry) = self.panels.queued.queue().entry(run) else { return };
        let State::Started { pane, server } = entry.state.clone() else { return };
        let label = entry.task.label.clone();
        let place = entry.task.place.clone();
        let workspace = place
            .workspace_id
            .and_then(|id| self.projects.workspace_by(id))
            .map(crate::project::Workspace::label)
            .or(place.workspace_name)
            .or(place.branch)
            .unwrap_or_default();
        self.panels.queued.set_state(run, State::Kept { pane, server, reason: reason.to_string() });
        let text = format!("routine {label} left changes in {workspace}: {reason}");
        self.messages.toast = Some(Toast::new(&text, ui::ToastIcon::Bug));
        self.outbox.notify(crate::host::notify::Notification::new(&text, &self.config.desktop_notifications));
        self.announce(&text);
        self.sync_queued();
    }
}

fn delete_clean_branch(repo: &std::path::Path, base: Option<&str>, branch: &str) {
    let clean = worktrees::scan::branch_commits(repo, branch, base).is_ok_and(|found| found.unpushed == 0);
    if !(clean && worktrees::remove::delete_branch(repo, branch).is_ok()) {
        crate::log::info!("routine", "branch kept", branch = branch);
    }
}
