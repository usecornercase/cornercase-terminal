use std::path::PathBuf;
use std::time::Instant;

use self::said::{Outcome, TreeFreed, builds_said};
use crate::app::App;
use crate::app::closed_agents::{Plan, closed_and, closed_message};
use crate::app::control::Asker;
use crate::app::control::worktrees::Freeing;
use crate::app::event::AppEvent;
use crate::app::notifications::{Message, MessageKind};
use crate::app::toast::{Toast, Undo};
use crate::app::worktrees::WorktreeEvent;
use crate::control::contract::Refusal;
use crate::error::Result;
use crate::flows::run::free_due;
use crate::log::{self, Job, Level};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::ui;

mod said;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub(crate) struct BuildsToFree {
    workspace: u64,
    tabs: Vec<u64>,
    agents: Vec<u64>,
    merged: String,
    trees: Vec<(Option<String>, PathBuf)>,
    closed: Instant,
}

#[derive(Debug)]
pub(crate) struct MergedBuilds {
    entry: BuildsToFree,
    keys: Vec<u64>,
    result: Result<Vec<TreeFreed>>,
}

impl App {
    pub(in crate::app) fn close_merged_runs(&mut self, now: Instant) {
        let ids: Vec<u64> = self.runs.all().iter().map(|run| run.id).collect();
        for id in ids {
            let plan = self.ready_to_close(id, now);
            let due = self.runs.run_mut(id).is_some_and(|run| run.close_due(plan.is_some(), now));
            if let Some(plan) = plan.filter(|_| due) {
                self.close_merged(id, &plan, now);
            }
        }
        self.free_merged_builds(now);
    }

    fn ready_to_close(&self, id: u64, now: Instant) -> Option<Plan> {
        let run = self.runs.get(id)?;
        let waiting = !run.due().is_empty() || self.runs.waking(id);
        if !run.close_when_merged || waiting || self.cleaning_up(run.workspace) {
            return None;
        }
        if !run.merged(self.pull_request(run.workspace)) {
            return None;
        }
        let plan = self.agents_close_plan(run.workspace).ok()?;
        let team_only = plan.fresh.iter().all(|pane| run.role_of(*pane).is_some());
        let calm = plan.cut.is_empty() && plan.others.is_empty() && team_only;
        (calm && !plan.agents.iter().any(|pane| self.pane_busy(*pane, now))).then_some(plan)
    }

    fn pane_busy(&self, pane: u64, now: Instant) -> bool {
        let working = self.projects.pane_by(pane).is_some_and(|term| self.holds_restart(term, now));
        working || self.person_holds(pane, now) || self.agents.launches.of_pane(pane).next().is_some()
    }

    fn close_merged(&mut self, id: u64, plan: &Plan, now: Instant) {
        let Some(at) = self.projects.workspace_index(plan.place.1) else { return };
        let merged = format!("{} merged in {}", self.pull_noun_of(at.project()), self.projects.workspace(at).label());
        let mut free = false;
        if let Some(run) = self.runs.run_mut(id) {
            free = run.free_builds;
            run.close_when_merged = false;
            run.free_builds = false;
        }
        log::info!(
            "flow",
            "agents closed after the merge",
            workspace = plan.place.1,
            run = id,
            agents = plan.agents.len(),
            fresh = plan.fresh.len(),
            free_builds = free
        );
        if free {
            return self.close_to_free(at, plan, merged, now);
        }
        let said = format!("{merged}: {}", closed_message(plan.agents.len()));
        self.close_agents_saying(plan, said.clone());
        let panes = plan.agents.clone();
        self.publish_message(Message {
            kind: MessageKind::FlowMerged,
            text: said,
            workspace: Some(plan.place.1),
            panes,
        });
    }

    fn close_to_free(&mut self, at: WorkspaceAt, plan: &Plan, merged: String, closed: Instant) {
        let workspace = self.projects.workspace(at);
        let own = workspace.worktree.then(|| (None, workspace.path.clone()));
        let roles = workspace.role_worktrees.iter().map(|tree| (Some(tree.role.clone()), tree.path.clone()));
        let trees = own.into_iter().chain(roles).collect();
        let said = format!("{merged}: {}", closed_and(plan.agents.len(), Some("freeing the builds")));
        self.close_agents_saying(plan, said);
        let (workspace, tabs, agents) = (plan.place.1, plan.tabs.clone(), plan.agents.clone());
        self.runs.wait_to_free(BuildsToFree { workspace, tabs, agents, merged, trees, closed });
    }

    fn free_merged_builds(&mut self, now: Instant) {
        let projects = &self.projects;
        let due = self.runs.due_frees(|entry| {
            let gone = entry.tabs.iter().all(|tab| projects.tab_index(*tab).is_none());
            free_due(entry.closed, now, gone)
        });
        for entry in due {
            self.start_free(entry);
        }
    }

    fn start_free(&mut self, entry: BuildsToFree) {
        let mut keys = Vec::new();
        let mut targets = Vec::new();
        for (role, path) in &entry.trees {
            let target = self.freeing(Asker::default(), None, Some(path), false);
            if let Ok(freeing) = &target {
                let key = self.clients.requests.key();
                self.git.frees.request_frees(key, freeing.path.clone());
                keys.push(key);
            }
            targets.push((role.clone(), path.clone(), target));
        }
        let job = Job::new(Level::Info, "flow", "free builds after the merge")
            .with("workspace", entry.workspace)
            .with("worktrees", entry.trees.len());
        self.spawn_job(
            job,
            move || Ok(targets.into_iter().map(freed_tree).collect::<Vec<_>>()),
            move |result, _| {
                let done = MergedBuilds { entry, keys, result };
                Some(AppEvent::Worktree(WorktreeEvent::MergedBuilds(Box::new(done))))
            },
        );
    }

    pub(in crate::app) fn merged_builds_freed(&mut self, done: MergedBuilds) {
        let MergedBuilds { entry, keys, result } = done;
        for key in keys {
            self.git.frees.request_ended(key);
        }
        let (clause, failed) = match &result {
            Ok(trees) => builds_said(trees),
            Err(e) => (format!("could not free the builds: {e}"), true),
        };
        let text = format!("{}: {}", entry.merged, closed_and(entry.agents.len(), Some(&clause)));
        let mut toast = Toast::new(text.clone(), if failed { ui::ToastIcon::Bug } else { ui::ToastIcon::Check });
        if self.agents.closed.has_run(entry.workspace) {
            toast.undo = Some(Undo::Agents(entry.workspace));
        }
        self.messages.toast = Some(toast);
        log::info!("flow", "builds freed after the merge", workspace = entry.workspace, failed = failed);
        let (workspace, panes) = (Some(entry.workspace), entry.agents);
        self.publish_message(Message { kind: MessageKind::FlowMerged, text, workspace, panes });
    }
}

fn freed_tree((role, path, target): (Option<String>, PathBuf, std::result::Result<Freeing, Refusal>)) -> TreeFreed {
    let outcome = match target.map_err(|refusal| refusal.message) {
        Err(message) => Outcome::Failed(message),
        Ok(freeing) => match freeing.unless_risky() {
            Ok(Ok(freed)) => Outcome::Freed(freed),
            Ok(Err(risky)) => Outcome::Kept(risky),
            Err(e) => Outcome::Failed(e.to_string()),
        },
    };
    TreeFreed { role, path, outcome }
}
