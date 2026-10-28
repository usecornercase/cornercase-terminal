use std::path::{Path, PathBuf};

use super::{Asker, Handled, conflict, json, malformed};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::worktrees::{Measure, WorktreeEvent};
use crate::app::worktrees_dialog::canonical;
use crate::control::contract::{Code, Refusal};
use crate::control::{BuildsFreed, FreeBuilds, WorktreeSize, WorktreeSized};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::process;
use crate::process::owners::trees;
use crate::project::{Hierarchy, WorkspaceAt};
use crate::term::Term;
use crate::worktrees::builds::{self, Size};
use crate::worktrees::{Kind, discover, scan};

struct Target {
    path: PathBuf,
    repo: PathBuf,
}

impl App {
    fn worktree_target(
        &self,
        asker: Asker,
        workspace: Option<u64>,
        path: Option<&Path>,
    ) -> std::result::Result<Target, Refusal> {
        match (workspace, path) {
            (Some(_), Some(_)) => Err(malformed("take --workspace or --path, not both")),
            (Some(id), None) => self.worktree_of(id),
            (None, Some(path)) => linked_target(path),
            (None, None) => {
                let at = self.here(asker).workspace;
                let id = at.map(|at| self.projects.workspace(at).id);
                self.worktree_of(id.ok_or_else(|| conflict("no workspace is open: pass --workspace or --path"))?)
            }
        }
    }

    fn worktree_of(&self, id: u64) -> std::result::Result<Target, Refusal> {
        let at = self.usable_workspace(id)?;
        let workspace = self.projects.workspace(at);
        if !workspace.worktree {
            return Err(conflict(format!("workspace {id} has no worktree of its own: its folder is its project's")));
        }
        let path = canonical(&workspace.path);
        let repo = discover::linked(&path).unwrap_or_else(|| self.projects[at.project()].path.clone());
        Ok(Target { path, repo })
    }

    fn workspace_at_path(&self, path: &Path) -> Option<WorkspaceAt> {
        let found = self.projects.iter().flat_map(|p| &p.workspaces).find(|w| canonical(&w.path) == path)?;
        self.projects.workspace_index(found.id)
    }

    pub(super) fn worktree_size_request(&mut self, client: u64, asker: Asker, request: &WorktreeSize) -> Handled {
        let target = self.worktree_target(asker, request.workspace, request.path.as_deref())?;
        let names = self.config.project(&target.repo, self.home.as_deref()).builds();
        let key = self.clients.requests.key();
        let job = Job::new(Level::Debug, "control", "worktree size").with("path", target.path.display());
        let (at, path) = (target.path.clone(), target.path);
        self.spawn_job(
            job,
            move || builds::measure(&at, Kind::Linked, &names),
            move |size, job| {
                if let Ok(size) = &size {
                    job.note("bytes", size.bytes);
                }
                let answer = Measure::Request(key);
                Some(AppEvent::Worktree(WorktreeEvent::Measured { answer, path, size: Box::new(size) }))
            },
        );
        self.pend_reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn worktree_size_answered(&mut self, key: u64, path: PathBuf, size: Result<Size>) {
        self.answer_read(key, |_| {
            let size = size.map_err(|e| Refusal::new(Code::Failed, e.to_string()))?;
            Ok(json(&WorktreeSized::of(path, &size)))
        });
    }

    pub(super) fn free_builds_request(&mut self, client: u64, asker: Asker, request: &FreeBuilds) -> Handled {
        let target = self.worktree_target(asker, request.workspace, request.path.as_deref())?;
        let names = self.config.project(&target.repo, self.home.as_deref()).builds();
        let shells = self.shell_pids();
        let mut running = self.workspace_at_path(&target.path).map(|at| self.running_in(at)).unwrap_or_default();
        let key = self.clients.requests.key();
        let job = Job::new(Level::Info, "control", "free builds").with("path", target.path.display());
        let (path, request) = (target.path, request.clone());
        self.spawn_job(
            job,
            move || {
                let found = outside_names(&path, shells);
                running.extend(found);
                running.sort();
                running.dedup();
                free_or_refuse(&path, &names, &request, running)
            },
            move |result, _| {
                let result = result.unwrap_or_else(|e| Err(Refusal::new(Code::Failed, e.to_string())));
                Some(AppEvent::Worktree(WorktreeEvent::BuildsRequest { key, result: Box::new(result) }))
            },
        );
        self.pend_reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn builds_request_answered(
        &mut self,
        key: u64,
        result: std::result::Result<BuildsFreed, Refusal>,
    ) {
        self.answer_read(key, |_| result.map(|freed| json(&freed)));
    }

    fn running_in(&self, at: WorkspaceAt) -> Vec<String> {
        let terms = self.projects.workspace(at).terms();
        terms.filter(|t| !t.shell_in_foreground()).filter_map(|t| t.program(&self.config)).collect()
    }

    fn shell_pids(&self) -> Vec<i32> {
        let workspaces = self.projects.iter().flat_map(|project| &project.workspaces);
        workspaces.flat_map(|workspace| workspace.terms().filter_map(Term::shell_pid)).collect()
    }
}

fn linked_target(path: &Path) -> std::result::Result<Target, Refusal> {
    let path = path.canonicalize().map_err(|e| conflict(format!("{}: {e}", path.display())))?;
    let repo = discover::linked(&path)
        .ok_or_else(|| conflict(format!("{} is not a linked worktree of a repository", path.display())))?;
    Ok(Target { path, repo })
}

fn outside_names(path: &Path, shells: Vec<i32>) -> Vec<String> {
    let shells: Vec<(i32, Vec<usize>)> = shells.into_iter().map(|pid| (pid, Vec::new())).collect();
    let panes = trees(&shells);
    let server = i32::try_from(std::process::id()).unwrap_or_default();
    let found = scan::outside(&[path.to_path_buf()], &panes, server);
    found.into_iter().map(|(_, name, _)| name).filter(|name| !process::is_shell(name)).collect()
}

fn free_or_refuse(
    path: &Path,
    names: &[String],
    request: &FreeBuilds,
    running: Vec<String>,
) -> Result<std::result::Result<BuildsFreed, Refusal>> {
    let size = builds::measure(path, Kind::Linked, names)?;
    if request.check {
        return Ok(Ok(BuildsFreed::would(path.to_path_buf(), &size, running)));
    }
    if !request.force
        && let Some(why) = risks(&running, &size)
    {
        return Ok(Err(conflict(why)));
    }
    let chosen: Vec<PathBuf> = size.builds.iter().map(|folder| folder.path.clone()).collect();
    let freed = builds::free(path, names, &chosen)?;
    Ok(Ok(BuildsFreed::freed(path.to_path_buf(), &freed, running)))
}

fn risks(running: &[String], size: &Size) -> Option<String> {
    let mut why = Vec::new();
    if !running.is_empty() {
        why.push(format!("{} run in this worktree and may use its build folders", running.join(", ")));
    }
    why.extend(size.nested.iter().map(|path| {
        format!("{} is a git checkout and its changes would go too", path.parent().unwrap_or(path).display())
    }));
    (!why.is_empty()).then(|| format!("{}; pass --force to free them anyway", why.join("; ")))
}
