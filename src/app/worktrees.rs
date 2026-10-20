use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use super::App;
use super::event::AppEvent;
use super::event::Start;
use super::flow_runs::Made;
use super::overlay::Overlay;
use super::toast::Toast;
use crate::agents::launch::Launch;
use crate::error::Result;
use crate::flows::run::RoleWorktree;
use crate::git;
use crate::git::upstream;
use crate::git::worktree::{self, setup};
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, Phase, Tab, Workspace};
use crate::pull_request::PullRequest;
use crate::ui::{self};

#[derive(Default)]
pub(super) struct Upstream {
    pub(super) fetched: HashMap<u64, Instant>,
    pub(super) counting: HashSet<u64>,
    pub(super) counted: Option<Instant>,
    pub(super) unlisted: HashSet<u64>,
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
    Run { project: u64, result: Result<Made>, request: u64 },
    Checked { project: u64, workspace: u64, status: worktree::Status, request: Option<u64> },
    Removed { project: u64, workspace: u64, result: Result<Option<String>>, request: Option<u64> },
    Behind { project: u64, behind: Vec<(u64, u32)> },
    PullRequests { project: u64, found: Vec<(u64, String, Option<PullRequest>)>, error: Option<String> },
}

impl WorktreeEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Created { .. } => "worktree created",
            Self::Run { .. } => "run worktrees created",
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
            WorktreeEvent::Run { project, result, request } => self.run_worktrees_ready(request, project, result, area),
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
        if self.git.upstream.counted.is_some_and(|at| now.duration_since(at) < COUNT_BEHIND_EVERY) {
            return;
        }
        self.git.upstream.counted = Some(now);
        self.git.upstream.fetched.retain(|id, _| self.projects.project_index(*id).is_some());
        for project in &self.projects {
            let workspaces: Vec<(u64, PathBuf)> = project
                .workspaces
                .iter()
                .filter(|w| git::branch(&w.path).is_some())
                .map(|w| (w.id, w.path.clone()))
                .collect();
            if workspaces.is_empty() || !self.git.upstream.counting.insert(project.id) {
                continue;
            }
            let fetch =
                self.git.upstream.fetched.get(&project.id).is_none_or(|at| now.duration_since(*at) >= fetch_every);
            if fetch {
                self.git.upstream.fetched.insert(project.id, now);
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
        self.git.upstream.counting.remove(&project);
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
        let mut dirs = Vec::new();
        for p in 0..self.projects.len() {
            if !git::is_repo_root(&self.projects[p].path) {
                continue;
            }
            let Some(linked) = self.linked_worktrees(p) else { continue };
            dirs.append(&mut self.drop_gone_worktrees(p, &linked));
            let project = &mut self.projects[p];
            for workspace in &mut project.workspaces {
                workspace.role_worktrees.retain(|tree| tree.path.is_dir());
            }
            let missing: Vec<PathBuf> =
                linked.into_iter().filter(|path| project.workspaces.iter().all(|w| &w.path != path)).collect();
            for path in missing {
                let id = self.take_id();
                self.projects[p].workspaces.push(Workspace::new(id, path, None, true));
            }
        }
        self.remove_run_dirs(dirs);
    }

    fn linked_worktrees(&mut self, p: usize) -> Option<Vec<PathBuf>> {
        let project = &self.projects[p];
        let id = project.id;
        match git::registered_worktrees(&project.path) {
            Ok(linked) => {
                self.git.upstream.unlisted.remove(&id);
                Some(linked)
            }
            Err(e) => {
                if self.git.upstream.unlisted.insert(id) {
                    log::warning!(
                        "worktree",
                        "cannot list the linked worktrees, keeping the workspaces",
                        project = id,
                        error = e,
                    );
                }
                None
            }
        }
    }

    fn drop_gone_worktrees(&mut self, p: usize, linked: &[PathBuf]) -> Vec<PathBuf> {
        let project = &mut self.projects[p];
        let gone: Vec<usize> = (0..project.workspaces.len())
            .rev()
            .filter(|&w| {
                let ws = &project.workspaces[w];
                ws.worktree && ws.open() && ws.tabs.is_empty() && !linked.contains(&ws.path)
            })
            .collect();
        let mut dirs = Vec::new();
        for w in gone {
            self.panels.changes.forget(project.workspaces[w].id);
            dirs.append(&mut project.workspaces[w].run_dirs);
            project.remove_workspace(w);
        }
        dirs
    }

    pub(super) fn spawn_worktree(&self, p: usize, branch: String, start: Option<Start>, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, repo) = (project.id, project.path.clone());
        let path = worktree::checkout_path(&self.config.worktrees_dir(self.paths.home.as_deref()), &repo, &branch);
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
        self.open_workspace((p, w), start, area, true)
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
        place: (usize, usize),
        start: Option<Start>,
        area: Rect,
        setup: bool,
    ) -> Result<()> {
        let (p, w) = place;
        let workspace = &mut self.projects[p].workspaces[w];
        if let Some(start) = &start
            && workspace.name.is_none()
        {
            workspace.name = Some(start.name.clone());
        }
        if workspace.tabs.is_empty() {
            let accounts = self.start_accounts(start.as_ref());
            let t = if setup {
                self.push_setup_tab_in(place, area, None, accounts)?
            } else {
                self.push_tab_in(place, area, None, accounts)?
            };
            self.projects[p].workspaces[w].active = t;
            let term = self.projects[p].workspaces[w].tab().and_then(Tab::pane).map(|t| t.id);
            if let (Some(term), Some(start)) = (term, start) {
                self.agents.launches.push(Launch::new(term, start.spec, Instant::now()));
            }
        }
        self.projects[p].active = w;
        self.active = p;
        Ok(())
    }

    pub(super) fn role_owner(&self, p: usize, w: usize) -> Option<String> {
        let path = &self.projects[p].workspaces[w].path;
        let project = &self.projects[p];
        project.workspaces.iter().filter(|owner| owner.open()).find_map(|owner| {
            let tree = owner.role_worktrees.iter().find(|tree| &tree.path == path)?;
            self.runs.list.iter().any(|run| run.workspace == owner.id).then(|| {
                format!(
                    "this worktree belongs to `{}` in the flow running in {} › {}: it is removed with that workspace",
                    tree.role,
                    self.project_label(project),
                    owner.label()
                )
            })
        })
    }

    pub(super) fn ask_removal(&mut self, p: usize, w: usize) {
        if let Some(refusal) = self.role_owner(p, w) {
            self.messages.toast = Some(Toast::new(refusal, ui::ToastIcon::Bug));
            return;
        }
        let (project, workspace) = (self.projects[p].id, self.projects[p].workspaces[w].id);
        self.spawn_check(p, w, None);
        self.overlay = Some(Overlay::RemoveWorkspace { project, workspace, check: Check::Running, lock: None });
    }

    pub(super) fn spawn_check(&self, p: usize, w: usize, request: Option<u64>) {
        let project = &self.projects[p];
        let (id, workspace) = (project.id, project.workspaces[w].id);
        let path = project.workspaces[w].path.clone();
        let roles: Vec<(String, PathBuf)> =
            project.workspaces[w].role_worktrees.iter().map(|tree| (tree.role.clone(), tree.path.clone())).collect();
        let job = Job::new(Level::Info, "worktree", "status").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(checked_status(&path, &roles)),
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
        self.start_removal(project, workspace, RemoveOptions { force: check == Check::Changed, unlock }, None);
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
            self.start_removal(project, workspace, RemoveOptions::default(), None);
        }
    }

    pub(super) fn start_removal(
        &mut self,
        project: u64,
        workspace: u64,
        options: RemoveOptions,
        request: Option<u64>,
    ) -> bool {
        let Some((p, w)) = self.projects.workspace_in(project, workspace) else { return false };
        let target = &mut self.projects[p].workspaces[w];
        if !target.open() {
            return false;
        }
        target.start_removing();
        target.kill();
        self.panels.changes.forget(workspace);
        self.projects[p].step_off(w);
        let (repo, path) = (self.projects[p].path.clone(), self.projects[p].workspaces[w].path.clone());
        let roles = self.take_role_trees(p, w);
        let script = self.teardown_script(p);
        let branch = git::branch(&path).unwrap_or_default();
        let job = Job::new(Level::Info, "worktree", "remove").with("workspace", workspace).with("path", path.display());
        let job = job.with("force", options.force).with("unlock", options.unlock);
        let job = job.with("teardown", script.is_some()).with("roles", roles.len());
        let teardown = Teardown { repo, script, options };
        self.spawn_job(
            job,
            move || teardown.remove_all(&path, &branch, &roles),
            move |result, _| Some(AppEvent::Worktree(WorktreeEvent::Removed { project, workspace, result, request })),
        );
        true
    }

    fn take_role_trees(&mut self, p: usize, w: usize) -> Vec<RoleWorktree> {
        let trees = self.projects[p].workspaces[w].role_worktrees.clone();
        let project = &mut self.projects[p];
        let mut stopped = Vec::new();
        for tree in &trees {
            if let Some(row) = project.workspaces.iter_mut().find(|row| row.path == tree.path && row.open()) {
                row.kill();
                stopped.push(row.id);
            }
        }
        for id in stopped {
            self.panels.changes.forget(id);
        }
        trees
    }

    pub(super) fn worktree_removed(&mut self, project: u64, workspace: u64, result: Result<Option<String>>) {
        let Some(name) = self.settle_removal(project, workspace, result.is_ok()) else { return };
        let toast = match result {
            Ok(None) => Toast::new(format!("removed {name}"), ui::ToastIcon::Check),
            Ok(Some(why)) => Toast::new(format!("removed {name}, but its teardown failed: {why}"), ui::ToastIcon::Bug),
            Err(e) => Toast::new(format!("could not remove {name}: {e}"), ui::ToastIcon::Bug),
        };
        self.messages.toast = Some(toast);
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
        (project, workspace): (u64, u64),
        check: Check,
        lock: Option<&worktree::Lock>,
        submit: &'static str,
    ) -> ui::Overlay {
        let found = self.projects.workspace_in(project, workspace);
        let teardown = found.is_some_and(|(p, _)| self.teardown_script(p).is_some());
        let (message, note) = match lock {
            Some(lock) => (
                lock_message(&self.found_label(found), lock) + if teardown { " Its teardown runs first." } else { "" },
                lock_note(lock, check == Check::Changed).map(ui::Note::Error),
            ),
            None => (
                self.remove_question(found, teardown),
                match check {
                    Check::Confirmed => Some(ui::Note::Busy("checking…")),
                    Check::Changed => Some(ui::Note::Error(UNCOMMITTED.into())),
                    Check::Running | Check::Clean => None,
                },
            ),
        };
        ui::Overlay::Confirm(ui::Confirm { title: "Remove workspace", message, note, submit, extra: None })
    }

    fn found_label(&self, found: Option<(usize, usize)>) -> String {
        found.map(|(p, w)| self.projects[p].workspaces[w].label()).unwrap_or_default()
    }

    fn remove_question(&self, found: Option<(usize, usize)>, teardown: bool) -> String {
        let label = self.found_label(found);
        let path = found
            .map(|(p, w)| crate::text::display_path(&self.projects[p].workspaces[w].path, self.paths.home.as_deref()))
            .unwrap_or_default();
        let roles = found.map_or(0, |(p, w)| {
            self.projects[p].workspaces[w].role_worktrees.iter().filter(|tree| tree.path.is_dir()).count()
        });
        let kept = if roles == 0 { "The branch is kept." } else { "The branches are kept." };
        let with_roles = if roles == 0 { String::new() } else { format!(", with the worktrees of its {roles} roles") };
        let first = if teardown { "Its teardown runs first. " } else { "" };
        format!("Remove the workspace {label} and delete its worktree folder {path}{with_roles}? {first}{kept}")
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct RemoveOptions {
    pub(super) force: bool,
    pub(super) unlock: bool,
}

struct Teardown {
    repo: PathBuf,
    script: Option<String>,
    options: RemoveOptions,
}

impl Teardown {
    fn remove_tree(&self, path: &Path, branch: &str) -> Result<Option<String>> {
        let failed = self
            .script
            .as_deref()
            .and_then(|script| setup::teardown(script, path, &self.repo, branch, setup::TEARDOWN_WITHIN));
        if let Some(why) = &failed {
            crate::log::debug!("worktree", "teardown failed", why = why);
        }
        worktree::remove(&self.repo, path, self.options.force, self.options.unlock).map(|()| failed)
    }

    fn remove_roles(&self, roles: &[RoleWorktree]) -> Vec<String> {
        let mut warnings = Vec::new();
        for tree in roles.iter().filter(|tree| tree.path.is_dir()) {
            match self.remove_tree(&tree.path, &tree.branch) {
                Ok(None) => {}
                Ok(Some(why)) => warnings.push(format!("the teardown of `{}` failed: {why}", tree.role)),
                Err(e) => warnings.push(format!(
                    "could not remove the worktree of `{}`, which stays in {}: {e}",
                    tree.role,
                    tree.path.display()
                )),
            }
        }
        warnings
    }

    fn remove_all(&self, path: &Path, branch: &str, roles: &[RoleWorktree]) -> Result<Option<String>> {
        let warnings = self.remove_roles(roles);
        let failed = self.remove_tree(path, branch)?;
        let warnings: Vec<String> = failed.into_iter().chain(warnings).collect();
        Ok((!warnings.is_empty()).then(|| warnings.join("; ")))
    }
}

fn checked_status(path: &Path, roles: &[(String, PathBuf)]) -> worktree::Status {
    let mut status = worktree::status(path);
    for (name, role) in roles.iter().filter(|(_, role)| role.is_dir()) {
        let found = worktree::status(role);
        status.changed |= found.changed;
        if status.lock.is_none() {
            status.lock = found.lock.map(|lock| worktree::Lock { role: Some((name.clone(), role.clone())), ..lock });
        }
    }
    status
}

pub(super) fn lock_message(label: &str, lock: &worktree::Lock) -> String {
    let reason = crate::text::truncate_right(&lock.reason, MAX_LOCK_REASON);
    let locked = if reason.is_empty() { "is locked".to_string() } else { format!("is locked: {reason}") };
    match &lock.role {
        Some((name, path)) => format!(
            "The worktree of `{name}` ({}) {locked}. Unlock it and remove {label} with its worktrees? The branches are kept.",
            path.display()
        ),
        None => format!("The worktree of {label} {locked}. Unlock it and delete its folder? The branch is kept."),
    }
}

pub(super) fn lock_note(lock: &worktree::Lock, changed: bool) -> Option<String> {
    let holder = lock.holder.map(|holder| match holder {
        worktree::Holder::Gone(pid) => format!("Process {pid} is gone; the lock was left behind."),
        worktree::Holder::Running(pid) => format!("Process {pid} still runs and may be using it."),
    });
    let parts: Vec<String> = holder.into_iter().chain(changed.then(|| UNCOMMITTED_TOO.to_string())).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}
