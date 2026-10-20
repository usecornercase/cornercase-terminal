use std::path::{Path, PathBuf};

use super::Overlay;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::worktrees::{WorktreeEvent, checked_status};
use crate::git::worktree;
use crate::log::{Job, Level};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::pull_request::State;
use crate::worktrees::{discover, scan};

const WILL_BE_LOST: &str = "its commits will be lost";

pub(in crate::app) const MAY_BE_LOST: &str = "its commits may be lost";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tree {
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) lost: Option<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Findings {
    pub(crate) changed: bool,
    pub(crate) lock: Option<worktree::Lock>,
    pub(crate) trees: Vec<Tree>,
}

impl Findings {
    pub(crate) fn shown(&self, branches: bool) -> Vec<(PathBuf, String)> {
        let trees = self.trees.iter().filter(|_| branches);
        trees.filter_map(|tree| Some((tree.path.clone(), tree.branch.clone()?))).collect()
    }

    pub(super) fn riskier_than(&self, shown: &Self, branches: bool) -> bool {
        let lost = |found: &Self| -> Vec<Option<String>> {
            found.trees.iter().filter(|tree| tree.lost.is_some()).map(|tree| tree.branch.clone()).collect()
        };
        let names = |found: &Self| -> Vec<Option<String>> { found.trees.iter().map(|t| t.branch.clone()).collect() };
        let more_lost = lost(self).iter().any(|branch| !lost(shown).contains(branch));
        (self.changed && !shown.changed)
            || self.lock != shown.lock
            || branches && (more_lost || names(self) != names(shown))
    }
}

#[derive(Clone)]
struct Wanted {
    path: PathBuf,
    branch: Option<String>,
    base: Option<String>,
    merged: bool,
}

impl Wanted {
    fn scanned(&self, default: Option<&str>) -> Tree {
        if !self.path.is_dir() {
            return self.unknown();
        }
        let Ok(found) = scan::scan(&self.path, self.base.as_deref().or(default)) else { return self.unknown() };
        let lost = found.unpushed > 0 && !(self.merged || found.merged && found.has_upstream);
        Tree { path: self.path.clone(), branch: found.branch, lost: lost.then_some(WILL_BE_LOST) }
    }

    fn unknown(&self) -> Tree {
        Tree { path: self.path.clone(), branch: self.branch.clone(), lost: Some(MAY_BE_LOST) }
    }
}

struct Checking {
    path: PathBuf,
    roles: Vec<(String, PathBuf)>,
    repo: PathBuf,
    wanted: Vec<Wanted>,
}

impl Checking {
    fn run(&self) -> Findings {
        let status = checked_status(&self.path, &self.roles);
        let default = crate::changes::git::default_base(&self.repo);
        let trees = self.wanted.iter().map(|want| want.scanned(default.as_deref())).collect();
        Findings { changed: status.changed, lock: status.lock, trees }
    }

    fn failed(&self) -> Findings {
        Findings { changed: true, lock: None, trees: self.wanted.iter().map(Wanted::unknown).collect() }
    }
}

impl App {
    pub(in crate::app) fn spawn_close_check(&mut self, at: WorkspaceAt) -> u64 {
        let generation = self.take_id();
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        let roles = workspace.role_worktrees.iter().map(|tree| (tree.role.clone(), tree.path.clone())).collect();
        let checking =
            Checking { path: workspace.path.clone(), roles, repo: project.path.clone(), wanted: self.wanted(at) };
        let job = Job::new(Level::Info, "worktree", "close check").with("workspace", workspace.id);
        let job = job.with("trees", checking.wanted.len());
        let fallback = checking.failed();
        self.spawn_job(
            job,
            move || Ok(checking.run()),
            move |found, job| {
                let found = found.unwrap_or_else(|_| fallback.clone());
                job.note("changed", found.changed);
                job.note("locked", found.lock.is_some());
                Some(AppEvent::Worktree(WorktreeEvent::AgentChecked { generation, found }))
            },
        );
        generation
    }

    fn wanted(&self, at: WorkspaceAt) -> Vec<Wanted> {
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        let row = |path: &Path| project.workspaces.iter().find(|row| row.path == path);
        let merged = |id: u64| self.pull_request(id).is_some_and(|pull| pull.state == State::Merged);
        let own = Wanted {
            path: workspace.path.clone(),
            branch: discover::branch_at(&workspace.path),
            base: workspace.base.clone(),
            merged: merged(workspace.id),
        };
        let roles = workspace.role_worktrees.iter().map(|tree| Wanted {
            path: tree.path.clone(),
            branch: Some(tree.branch.clone()),
            base: row(&tree.path).and_then(|row| row.base.clone()),
            merged: row(&tree.path).is_some_and(|row| merged(row.id)),
        });
        std::iter::once(own).chain(roles).collect()
    }

    pub(in crate::app) fn close_agent_checked(&mut self, generation: u64, found: Findings) {
        let Some(Overlay::CloseAgent(dialog)) = &mut self.overlay else { return };
        if dialog.generation != generation || dialog.found.is_some() {
            return;
        }
        dialog.found = Some(found);
        self.close_agent_answered();
    }
}
