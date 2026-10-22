use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::canonical;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::app::worktrees::{RemoveOptions, WorktreeEvent};
use crate::error::Result;
use crate::flows::run::RoleWorktree;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::text::plural;
use crate::ui;
use crate::worktrees::{Row, Step, discover, remove};

#[derive(Default)]
pub(in crate::app) struct Cleanup {
    pub(in crate::app) pending: HashMap<u64, Pending>,
    tally: Tally,
}

#[derive(Debug, Default)]
struct Tally {
    removed: usize,
    deleted: Vec<String>,
    notes: Vec<String>,
    failed: Vec<String>,
}

impl Tally {
    fn add(&mut self, other: Self) {
        self.removed += other.removed;
        self.deleted.extend(other.deleted);
        self.notes.extend(other.notes);
        self.failed.extend(other.failed);
    }
}

pub(in crate::app) struct Pending {
    name: String,
    repo: PathBuf,
    paths: Vec<PathBuf>,
    branches: Vec<String>,
    removed: bool,
}

pub(in crate::app) type Deleted = Vec<(String, Result<()>)>;

impl Pending {
    fn new(name: String, repo: PathBuf, paths: Vec<PathBuf>, branches: Vec<String>) -> Self {
        Self { name, repo, paths, branches, removed: false }
    }
}

impl App {
    pub(super) fn remove_worktrees(&mut self, branches: bool) {
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        if !self.worktrees_ready(dialog) {
            return;
        }
        let rows = dialog.list.rows.clone();
        let picked: Vec<&Row> = rows.iter().filter(|row| row.ticked && row.tickable()).collect();
        let covered: HashSet<PathBuf> = picked
            .iter()
            .filter_map(|row| self.live(&row.found.path))
            .flat_map(|(at, _)| self.projects.workspace(at).role_worktrees.iter().map(|t| canonical(&t.path)))
            .collect();
        let mut removing: Vec<PathBuf> = Vec::new();
        let mut skipped = Vec::new();
        for row in picked {
            if covered.contains(&row.found.path) || removing.contains(&row.found.path) {
                continue;
            }
            match self.remove_one(row, &rows, branches) {
                Ok(paths) => removing.extend(paths),
                Err(why) => skipped.push(why),
            }
        }
        if removing.is_empty() {
            let why = skipped.first().copied().unwrap_or("nothing ticked can be removed");
            self.messages.toast = Some(Toast::new(format!("nothing was removed: {why}"), ui::ToastIcon::Bug));
            self.git.cleanup.tally.notes.clear();
        }
        if let Some(Overlay::Worktrees(dialog)) = &mut self.overlay {
            dialog.step = Step::List;
            for row in dialog.list.rows.iter_mut().filter(|row| removing.contains(&row.found.path)) {
                row.ticked = false;
                row.removing = true;
            }
        }
    }

    fn remove_one(
        &mut self,
        row: &Row,
        rows: &[Row],
        branches: bool,
    ) -> std::result::Result<Vec<PathBuf>, &'static str> {
        match self.live(&row.found.path) {
            Some((_, live)) if live.removing => Err("it is already being removed or closed"),
            Some((_, live)) if live.role_of_run => Err("it belongs to a running flow"),
            Some((at, _)) if !self.projects.workspace(at).worktree => Err("it is a project's own folder"),
            Some((at, _)) => self.remove_open(row, rows, branches, at),
            None => Ok(self.remove_closed(row, branches)),
        }
    }

    fn remove_open(
        &mut self,
        row: &Row,
        rows: &[Row],
        branches: bool,
        at: WorkspaceAt,
    ) -> std::result::Result<Vec<PathBuf>, &'static str> {
        let roles = self.projects.workspace(at).role_worktrees.clone();
        let paths = self.removal_paths(at);
        let (mut names, before) = (Vec::new(), self.git.cleanup.tally.notes.len());
        if branches {
            names.extend(self.deletable(row));
            names.extend(self.role_branches(rows, &roles));
        }
        if !self.remove_workspace_with_branches(at, (row_name(row), row.found.repo.clone()), names, (true, true)) {
            self.git.cleanup.tally.notes.truncate(before);
            return Err("its workspace could not be stopped");
        }
        Ok(paths)
    }

    fn removal_paths(&self, at: WorkspaceAt) -> Vec<PathBuf> {
        let workspace = self.projects.workspace(at);
        let mut paths = vec![canonical(&workspace.path)];
        paths.extend(workspace.role_worktrees.iter().map(|tree| canonical(&tree.path)));
        paths
    }

    pub(in crate::app) fn remove_workspace_with_branches(
        &mut self,
        at: WorkspaceAt,
        (name, repo): (String, PathBuf),
        branches: Vec<String>,
        (force, unlock): (bool, bool),
    ) -> bool {
        let paths = self.removal_paths(at);
        let (project, workspace) = (self.projects[at.project()].id, self.projects.workspace(at).id);
        if !self.start_removal(project, workspace, RemoveOptions { force, unlock }, None) {
            return false;
        }
        self.git.cleanup.pending.insert(workspace, Pending::new(name, repo, paths, branches));
        true
    }

    fn role_branches(&mut self, rows: &[Row], roles: &[RoleWorktree]) -> Vec<String> {
        let mut names = Vec::new();
        for tree in roles {
            let path = canonical(&tree.path);
            let (shown, now) = match rows.iter().find(|r| r.found.path == path) {
                Some(role) => (role.branch().map(str::to_string), discover::checked_out(&role.found)),
                None => (Some(tree.branch.clone()), branch_now(&path, &tree.branch)),
            };
            names.extend(self.keep_or_delete(shown, now.as_deref()));
        }
        names
    }

    fn remove_closed(&mut self, row: &Row, branches: bool) -> Vec<PathBuf> {
        let found = &row.found;
        let key = self.take_id();
        let names: Vec<String> = if branches { self.deletable(row).into_iter().collect() } else { Vec::new() };
        let script = self.config.project(&found.repo, self.home.as_deref()).teardown;
        let script = (!script.trim().is_empty()).then_some(script);
        let job = Job::new(Level::Info, "worktrees", "remove").with("path", found.path.display());
        let job = job.with("kind", format!("{:?}", found.kind)).with("branches", names.join(","));
        let (target, repo, deleting) = (found.clone(), found.repo.clone(), names.clone());
        self.spawn_job(
            job,
            move || {
                let warning = remove::remove(&target, script.as_deref())?;
                Ok((warning, delete(&repo, &deleting)))
            },
            move |result, _| {
                let (result, branch) = match result {
                    Ok((warning, deleted)) => (Ok(warning), deleted),
                    Err(e) => (Err(e), Vec::new()),
                };
                Some(AppEvent::Worktree(WorktreeEvent::Cleaned { key, result, branch }))
            },
        );
        let paths = vec![found.path.clone()];
        self.git.cleanup.pending.insert(key, Pending::new(row_name(row), found.repo.clone(), paths.clone(), names));
        paths
    }

    fn deletable(&mut self, row: &Row) -> Option<String> {
        self.keep_or_delete(row.branch().map(str::to_string), discover::checked_out(&row.found).as_deref())
    }

    fn keep_or_delete(&mut self, shown: Option<String>, now: Option<&str>) -> Option<String> {
        let shown = shown?;
        if now == Some(shown.as_str()) {
            return Some(shown);
        }
        log::info!("worktrees", "branch kept: it changed since the confirmation", branch = shown);
        self.git.cleanup.tally.notes.push(format!("kept the branch {shown}: it changed since the confirmation"));
        None
    }

    pub(in crate::app) fn cleaning(&self, workspace: u64) -> bool {
        self.git.cleanup.pending.contains_key(&workspace)
    }

    pub(super) fn cleaning_path(&self, path: &Path) -> bool {
        self.git.cleanup.pending.values().any(|pending| pending.paths.iter().any(|p| p == path))
    }

    pub(in crate::app) fn cleanup_removed(&mut self, project: u64, workspace: u64, result: Result<Option<String>>) {
        self.settle_removal(project, workspace, result.is_ok());
        let ok = result.is_ok();
        self.cleaned(workspace, result, Vec::new());
        let Some(pending) = self.git.cleanup.pending.get(&workspace) else { return };
        if !ok || pending.branches.is_empty() {
            self.cleanup_finished(workspace);
            return;
        }
        let (repo, names) = (pending.repo.clone(), pending.branches.clone());
        let job = Job::new(Level::Info, "worktrees", "delete branches").with("branches", names.join(","));
        self.spawn_job(
            job,
            move || Ok(delete(&repo, &names)),
            move |deleted, _| {
                Some(AppEvent::Worktree(WorktreeEvent::Cleaned {
                    key: workspace,
                    result: Ok(None),
                    branch: deleted.unwrap_or_default(),
                }))
            },
        );
    }

    pub(in crate::app) fn worktree_cleaned(&mut self, key: u64, result: Result<Option<String>>, branch: Deleted) {
        self.cleaned(key, result, branch);
        self.cleanup_finished(key);
    }

    fn cleanup_finished(&mut self, key: u64) {
        if self.git.cleanup.pending.remove(&key).is_some() {
            self.finish_cleanup();
        }
    }

    fn cleaned(&mut self, key: u64, result: Result<Option<String>>, branch: Deleted) {
        let Some(pending) = self.git.cleanup.pending.get_mut(&key) else { return };
        let mut tally = Tally::default();
        if !pending.removed {
            pending.removed = true;
            let (name, paths) = (pending.name.clone(), pending.paths.clone());
            self.tally_removal(&mut tally, &name, &paths, result);
        }
        for (branch, deleted) in branch {
            match deleted {
                Ok(()) => tally.deleted.push(branch),
                Err(e) => tally.failed.push(format!("could not delete the branch {branch}: {e}")),
            }
        }
        self.git.cleanup.tally.add(tally);
    }

    fn tally_removal(&mut self, tally: &mut Tally, name: &str, paths: &[PathBuf], result: Result<Option<String>>) {
        match result {
            Ok(warning) => {
                tally.failed.extend(warning.map(|why| format!("{name}: {why}")));
                let gone: Vec<PathBuf> = paths
                    .iter()
                    .enumerate()
                    .filter(|(i, path)| *i == 0 || !path.exists())
                    .map(|(_, path)| path.clone())
                    .collect();
                tally.removed += gone.len();
                self.worktree_rows_back(paths);
                self.worktree_rows_gone(&gone);
            }
            Err(e) => {
                tally.failed.push(format!("could not remove {name}: {e}"));
                self.worktree_rows_back(paths);
            }
        }
    }

    fn worktree_rows_gone(&mut self, paths: &[PathBuf]) {
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        for row in dialog.list.rows.iter_mut().filter(|row| paths.contains(&row.found.path)) {
            row.gone = true;
            row.ticked = false;
            row.removing = false;
        }
    }

    fn worktree_rows_back(&mut self, paths: &[PathBuf]) {
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        for row in dialog.list.rows.iter_mut().filter(|row| paths.contains(&row.found.path)) {
            row.removing = false;
        }
    }

    fn finish_cleanup(&mut self) {
        if !self.git.cleanup.pending.is_empty() {
            return;
        }
        let Tally { removed, deleted, notes, failed } = std::mem::take(&mut self.git.cleanup).tally;
        if removed == 0 && failed.is_empty() && notes.is_empty() {
            return;
        }
        let mut parts = vec![format!("removed {}", plural(removed, "worktree", "worktrees"))];
        if !deleted.is_empty() {
            parts[0] = format!("{} and {}", parts[0], plural(deleted.len(), "branch", "branches"));
        }
        parts.extend(notes);
        parts.extend(failed.iter().cloned());
        let mut text = parts.join("; ");
        let icon = if failed.is_empty() {
            ui::ToastIcon::Check
        } else {
            text.push_str(", see server.log");
            ui::ToastIcon::Bug
        };
        self.messages.toast = Some(Toast::new(text, icon));
    }
}

fn branch_now(path: &Path, shown: &str) -> Option<String> {
    if path.exists() { discover::branch_at(path) } else { Some(shown.to_string()) }
}

fn delete(repo: &Path, branches: &[String]) -> Deleted {
    branches.iter().map(|branch| (branch.clone(), remove::delete_branch(repo, branch))).collect()
}

fn row_name(row: &Row) -> String {
    row.branch().map_or_else(|| row.found.path.display().to_string(), str::to_string)
}
