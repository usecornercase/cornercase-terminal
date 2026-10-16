use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::event::AppEvent;
use super::event::Start;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents::launch::Launch;
use crate::error::Result;
use crate::git;
use crate::git::upstream;
use crate::git::worktree::{self, setup};
use crate::log::{Job, Level};
use crate::project::{Hierarchy, Phase, Tab, Workspace};
use crate::pull_request::PullRequest;
use crate::ui::{self};

#[derive(Default)]
pub(super) struct Upstream {
    pub(super) fetched: HashMap<u64, Instant>,
    pub(super) counting: HashSet<u64>,
    pub(super) counted: Option<Instant>,
}

pub(super) const UNCOMMITTED: &str = "It has changes that are not committed; removing it deletes them.";

const UNCOMMITTED_TOO: &str = "It has uncommitted changes, which are deleted.";

const MAX_LOCK_REASON: usize = 100;

pub(super) const SYNC_EVERY: Duration = Duration::from_secs(1);

pub(super) const COUNT_BEHIND_EVERY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Check {
    Running,
    Confirmed,
    Clean,
    Changed,
}

#[derive(Debug)]
pub(crate) enum WorktreeEvent {
    Created { project: u64, result: Result<PathBuf>, start: Option<Start>, request: Option<u64> },
    Checked { project: u64, workspace: u64, status: worktree::Status, request: Option<u64> },
    Removed { project: u64, workspace: u64, result: Result<Option<String>>, request: Option<u64> },
    Behind { project: u64, behind: Vec<(u64, u32)> },
    PullRequests { project: u64, found: Vec<(u64, String, Option<PullRequest>)>, error: Option<String> },
}

impl WorktreeEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Created { .. } => "worktree created",
            Self::Checked { .. } => "worktree checked",
            Self::Removed { .. } => "worktree removed",
            Self::Behind { .. } => "behind",
            Self::PullRequests { .. } => "pull requests",
        }
    }
}

impl App {
    pub(super) fn worktree_answered(&mut self, event: WorktreeEvent, area: Rect) -> Result<()> {
        match event {
            WorktreeEvent::Created { project, result, request: Some(key), .. } => {
                self.worktree_ready(key, project, result, area);
            }
            WorktreeEvent::Created { project, result, start, request: None } => {
                self.worktree_created(project, result, start, area)?;
            }
            WorktreeEvent::Checked { project, workspace, status, request: Some(key) } => {
                self.removal_checked(key, project, workspace, &status);
            }
            WorktreeEvent::Checked { project, workspace, status, request: None } => {
                self.worktree_checked(project, workspace, status);
            }
            WorktreeEvent::Removed { project, workspace, result, request: Some(key) } => {
                self.worktree_gone(key, project, workspace, result);
            }
            WorktreeEvent::Removed { project, workspace, result, request: None } => {
                self.worktree_removed(project, workspace, result);
            }
            WorktreeEvent::Behind { project, behind } => self.behind_counted(project, &behind),
            WorktreeEvent::PullRequests { project, found, error } => self.pull_requests_found(project, found, error),
        }
        Ok(())
    }

    pub(super) fn count_behind(&mut self, now: Instant) {
        let Some(fetch_every) = self.config.fetch_every() else {
            self.projects.workspaces_mut().for_each(|w| w.behind = 0);
            return;
        };
        if self.upstream.counted.is_some_and(|at| now.duration_since(at) < COUNT_BEHIND_EVERY) {
            return;
        }
        self.upstream.counted = Some(now);
        self.upstream.fetched.retain(|id, _| self.projects.project_index(*id).is_some());
        for project in &self.projects {
            let workspaces: Vec<(u64, PathBuf)> = project
                .workspaces
                .iter()
                .filter(|w| git::branch(&w.path).is_some())
                .map(|w| (w.id, w.path.clone()))
                .collect();
            if workspaces.is_empty() || !self.upstream.counting.insert(project.id) {
                continue;
            }
            let fetch = self.upstream.fetched.get(&project.id).is_none_or(|at| now.duration_since(*at) >= fetch_every);
            if fetch {
                self.upstream.fetched.insert(project.id, now);
            }
            let (id, repo) = (project.id, project.path.clone());
            let job = Job::new(Level::Debug, "git", "upstream").with("project", id).with("fetch", fetch);
            let checking = workspaces.clone();
            self.spawn_job(
                job,
                move || Ok(upstream::check(&repo, &checking, fetch)),
                move |behind, _| {
                    let behind = behind.unwrap_or_else(|_| workspaces.iter().map(|(id, _)| (*id, 0)).collect());
                    Some(AppEvent::Worktree(WorktreeEvent::Behind { project: id, behind }))
                },
            );
        }
    }

    pub(super) fn behind_counted(&mut self, project: u64, behind: &[(u64, u32)]) {
        self.upstream.counting.remove(&project);
        if self.config.fetch_every().is_none() {
            return;
        }
        let Some(p) = self.projects.project_index(project) else { return };
        let p = &mut self.projects[p];
        for (id, n) in behind {
            if let Some(w) = p.index_of(*id) {
                p.workspaces[w].behind = *n;
            }
        }
    }

    pub(super) fn sync_worktrees(&mut self) {
        for p in 0..self.projects.len() {
            let project = &mut self.projects[p];
            if !git::is_repo_root(&project.path) {
                continue;
            }
            let linked = git::linked_worktrees(&project.path);
            let gone: Vec<usize> = (0..project.workspaces.len())
                .rev()
                .filter(|&w| {
                    let ws = &project.workspaces[w];
                    ws.worktree && ws.open() && ws.tabs.is_empty() && !ws.path.is_dir()
                })
                .collect();
            for w in gone {
                self.changes.forget(project.workspaces[w].id);
                project.remove_workspace(w);
            }
            let missing: Vec<PathBuf> =
                linked.into_iter().filter(|path| project.workspaces.iter().all(|w| &w.path != path)).collect();
            for path in missing {
                let id = self.take_id();
                self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
            }
        }
    }

    pub(super) fn spawn_worktree(&self, p: usize, branch: String, start: Option<Start>, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, repo) = (project.id, project.path.clone());
        let path = worktree::checkout_path(&self.config.worktrees_dir(self.home.as_deref()), &repo, &branch);
        let job = Job::new(Level::Info, "worktree", "add").with("project", id).with("branch", &branch);
        let job = job.with("path", path.display()).with("issue", start.is_some());
        self.spawn_job(
            job,
            move || worktree::create(&repo, &branch, &path).map(|()| path),
            move |result, _| Some(AppEvent::Worktree(WorktreeEvent::Created { project: id, result, start, request })),
        );
    }

    pub(super) fn create_workspace(
        &mut self,
        project: u64,
        input: String,
        worktree: Option<bool>,
        area: Rect,
    ) -> Result<Option<Overlay>> {
        let name = input.trim().to_string();
        let Some(p) = self.projects.project_index(project) else { return Ok(None) };
        if name.is_empty() {
            let error = Some("the name is required".into());
            return Ok(Some(Overlay::NewWorkspace { project, input, worktree, error, creating: false }));
        }
        if worktree == Some(true) {
            self.spawn_worktree(p, name, None, None);
            return Ok(Some(Overlay::NewWorkspace { project, input, worktree, error: None, creating: true }));
        }
        let repo = self.projects[p].path.clone();
        let workspace = self.new_workspace(area, repo, Some(name), false)?;
        let project = &mut self.projects[p];
        project.workspaces.push(workspace);
        project.active = project.workspaces.len() - 1;
        self.active = p;
        Ok(None)
    }

    pub(super) fn worktree_created(
        &mut self,
        project: u64,
        result: Result<PathBuf>,
        start: Option<Start>,
        area: Rect,
    ) -> Result<()> {
        let path = match result {
            Ok(path) => path.canonicalize().unwrap_or(path),
            Err(e) => {
                match &mut self.overlay {
                    Some(Overlay::NewWorkspace { error, creating, .. }) => {
                        *error = Some(e.to_string());
                        *creating = false;
                    }
                    Some(Overlay::Issues(b)) => {
                        b.error = Some(e.to_string());
                        b.starting = false;
                    }
                    _ => {}
                }
                return Ok(());
            }
        };
        if matches!(self.overlay, Some(Overlay::NewWorkspace { .. } | Overlay::Issues(_))) {
            self.overlay = None;
        }
        let Some(p) = self.projects.project_index(project) else { return Ok(()) };
        let w = self.worktree_workspace(p, path);
        self.open_workspace(p, w, start, area, true)
    }

    pub(super) fn worktree_workspace(&mut self, p: usize, path: PathBuf) -> usize {
        if let Some(w) = self.projects[p].workspaces.iter().position(|w| w.path == path) {
            return w;
        }
        let id = self.take_id();
        self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
        self.projects[p].workspaces.len() - 1
    }

    pub(super) fn open_workspace(
        &mut self,
        p: usize,
        w: usize,
        start: Option<Start>,
        area: Rect,
        setup: bool,
    ) -> Result<()> {
        let workspace = &mut self.projects[p].workspaces[w];
        if let Some(start) = &start
            && workspace.name.is_none()
        {
            workspace.name = Some(start.name.clone());
        }
        if workspace.tabs.is_empty() {
            if setup {
                let t = self.push_setup_tab(p, w, area, None)?;
                self.projects[p].workspaces[w].active = t;
            } else {
                self.add_tab(p, w, area)?;
            }
            let term = self.projects[p].workspaces[w].tab().and_then(Tab::pane).map(|t| t.id);
            if let (Some(term), Some(start)) = (term, start) {
                self.agents.launches.push(Launch::new(term, start.spec, Instant::now()));
            }
        }
        self.projects[p].active = w;
        self.active = p;
        Ok(())
    }

    pub(super) fn ask_removal(&mut self, p: usize, w: usize) {
        let (project, workspace) = (self.projects[p].id, self.projects[p].workspaces[w].id);
        self.spawn_check(p, w, None);
        self.overlay = Some(Overlay::RemoveWorkspace { project, workspace, check: Check::Running, lock: None });
    }

    pub(super) fn spawn_check(&self, p: usize, w: usize, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, workspace) = (project.id, project.workspaces[w].id);
        let path = project.workspaces[w].path.clone();
        let job = Job::new(Level::Info, "worktree", "status").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(worktree::status(&path)),
            move |status, job| {
                job.note("changed", status.as_ref().map_or(true, |s| s.changed));
                job.note("locked", status.as_ref().is_ok_and(|s| s.lock.is_some()));
                let status = status.unwrap_or(worktree::Status { changed: true, lock: None });
                Some(AppEvent::Worktree(WorktreeEvent::Checked { project: id, workspace, status, request }))
            },
        );
    }

    pub(super) fn confirm_removal(
        &mut self,
        project: u64,
        workspace: u64,
        check: Check,
        unlock: bool,
    ) -> Option<Overlay> {
        if matches!(check, Check::Running | Check::Confirmed) {
            return Some(Overlay::RemoveWorkspace { project, workspace, check: Check::Confirmed, lock: None });
        }
        self.start_removal(project, workspace, check == Check::Changed, unlock, None);
        None
    }

    pub(super) fn worktree_checked(&mut self, project: u64, workspace: u64, status: worktree::Status) {
        let Some(Overlay::RemoveWorkspace { project: asked, workspace: shown, check, lock }) = &mut self.overlay else {
            return;
        };
        if (*asked, *shown) != (project, workspace) || !matches!(check, Check::Running | Check::Confirmed) {
            return;
        }
        let go_on = *check == Check::Confirmed && !status.changed && status.lock.is_none();
        *check = if status.changed { Check::Changed } else { Check::Clean };
        *lock = status.lock;
        if go_on {
            self.overlay = None;
            self.start_removal(project, workspace, false, false, None);
        }
    }

    pub(super) fn start_removal(
        &mut self,
        project: u64,
        workspace: u64,
        force: bool,
        unlock: bool,
        request: Option<u64>,
    ) -> bool {
        let Some((p, w)) = self.projects.workspace_in(project, workspace) else { return false };
        let target = &mut self.projects[p].workspaces[w];
        if !target.open() {
            return false;
        }
        target.start_removing();
        target.kill();
        self.changes.forget(workspace);
        self.projects[p].step_off(w);
        let (repo, path) = (self.projects[p].path.clone(), self.projects[p].workspaces[w].path.clone());
        let teardown = self.teardown_script(p).map(|script| (script, git::branch(&path).unwrap_or_default()));
        let job = Job::new(Level::Info, "worktree", "remove").with("workspace", workspace).with("path", path.display());
        let job = job.with("force", force).with("unlock", unlock).with("teardown", teardown.is_some());
        self.spawn_job(
            job,
            move || {
                let failed = teardown.and_then(|(script, branch)| {
                    setup::teardown(&script, &path, &repo, &branch, setup::TEARDOWN_WITHIN)
                });
                if let Some(why) = &failed {
                    crate::log::debug!("worktree", "teardown failed", why = why);
                }
                worktree::remove(&repo, &path, force, unlock).map(|()| failed)
            },
            move |result, _| Some(AppEvent::Worktree(WorktreeEvent::Removed { project, workspace, result, request })),
        );
        true
    }

    pub(super) fn worktree_removed(&mut self, project: u64, workspace: u64, result: Result<Option<String>>) {
        let Some(name) = self.settle_removal(project, workspace, result.is_ok()) else { return };
        let toast = match result {
            Ok(None) => Toast::new(format!("removed {name}"), ui::ToastIcon::Check),
            Ok(Some(why)) => Toast::new(format!("removed {name}, but its teardown failed: {why}"), ui::ToastIcon::Bug),
            Err(e) => Toast::new(format!("could not remove {name}: {e}"), ui::ToastIcon::Bug),
        };
        self.toast = Some(toast);
    }

    pub(super) fn settle_removal(&mut self, project: u64, workspace: u64, removed: bool) -> Option<String> {
        let (p, w) = self.projects.workspace_in(project, workspace)?;
        let name = self.projects[p].workspaces[w].label();
        if removed {
            self.drop_workspace(project, workspace);
        } else {
            self.projects[p].workspaces[w].phase = Phase::Open;
        }
        Some(name)
    }

    pub(super) fn remove_view(
        &self,
        project: u64,
        workspace: u64,
        check: Check,
        lock: Option<&worktree::Lock>,
        submit: &'static str,
    ) -> ui::Overlay {
        let found = self.projects.workspace_in(project, workspace);
        let (label, path) = found
            .map(|(p, w)| {
                let ws = &self.projects[p].workspaces[w];
                (ws.label(), crate::text::display_path(&ws.path, self.home.as_deref()))
            })
            .unwrap_or_default();
        let teardown = found.is_some_and(|(p, _)| self.teardown_script(p).is_some());
        let (message, note) = match lock {
            Some(lock) => (
                lock_message(&label, lock) + if teardown { " Its teardown runs first." } else { "" },
                lock_note(lock, check == Check::Changed).map(ui::Note::Error),
            ),
            None => (
                format!(
                    "Remove the workspace {label} and delete its worktree folder {path}? {}The branch is kept.",
                    if teardown { "Its teardown runs first. " } else { "" }
                ),
                match check {
                    Check::Confirmed => Some(ui::Note::Busy("checking…")),
                    Check::Changed => Some(ui::Note::Error(UNCOMMITTED.into())),
                    Check::Running | Check::Clean => None,
                },
            ),
        };
        ui::Overlay::Confirm(ui::Confirm { title: "Remove workspace", message, note, submit, extra: None })
    }
}

pub(super) fn lock_message(label: &str, lock: &worktree::Lock) -> String {
    let reason = crate::text::truncate_right(&lock.reason, MAX_LOCK_REASON);
    let locked = if reason.is_empty() { "is locked".to_string() } else { format!("is locked: {reason}") };
    format!("The worktree of {label} {locked}. Unlock it and delete its folder? The branch is kept.")
}

pub(super) fn lock_note(lock: &worktree::Lock, changed: bool) -> Option<String> {
    let holder = lock.holder.map(|holder| match holder {
        worktree::Holder::Gone(pid) => format!("Process {pid} is gone; the lock was left behind."),
        worktree::Holder::Running(pid) => format!("Process {pid} still runs and may be using it."),
    });
    let parts: Vec<String> = holder.into_iter().chain(changed.then(|| UNCOMMITTED_TOO.to_string())).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}
