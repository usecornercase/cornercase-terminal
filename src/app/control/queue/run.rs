use std::time::Instant;

use super::super::flow::pick;
use crate::agents;
use crate::app::App;
use crate::app::flow_runs::Starting;
use crate::control;
use crate::git;
use crate::git::worktree;
use crate::queue::{self, Entry, State};

const BRANCHES_TRIED: usize = 1000;

impl App {
    pub(super) fn start_routine_now(&mut self, routine: u64, now: Instant) -> Result<u64, String> {
        let at = crate::time::now();
        let run = self.panels.queued.run_now(routine, at)?;
        if let Err(reason) = self.run_queued(run, now) {
            self.fail_queued(run, &reason);
        }
        Ok(run)
    }

    fn branch_taken(&self, p: usize, id: u64, name: &str) -> bool {
        let repo = &self.projects[p].path;
        let dir = self.config.worktrees_dir(self.home.as_deref());
        let starting = |entry: &Entry| {
            entry.id != id && entry.state == State::Starting && entry.task.place.branch.as_deref() == Some(name)
        };
        git::has_branch(repo, name)
            || self.workspace_on(p, name).is_some()
            || worktree::checkout_path(&dir, repo, name).exists()
            || self.panels.queued.queue().entries.iter().any(starting)
    }

    fn routine_branch(&self, p: usize, entry: &Entry) -> Result<String, String> {
        let task = &entry.task;
        let slug = crate::issues::slug(&task.label, 32);
        let slug = if slug.is_empty() { "routine".to_string() } else { slug };
        let base = format!("routine/{slug}-{}", queue::clock::stamp(task.at, &task.zone));
        let name = |n: usize| if n == 1 { base.clone() } else { format!("{base}-{n}") };
        (1..=BRANCHES_TRIED)
            .map(name)
            .find(|name| !self.branch_taken(p, entry.id, name))
            .ok_or_else(|| format!("no free branch name starting with {base}"))
    }

    pub(super) fn run_routine(&mut self, entry: &Entry, now: Instant) -> Result<(), String> {
        let (p, _) = self.queue_place(&entry.task.place)?;
        let branch = self.routine_branch(p, entry)?;
        self.panels.queued.set_branch(entry.id, &branch);
        match &entry.task.flow {
            Some(name) => self.run_routine_flow(entry, p, name, branch),
            None => self.run_routine_agent(entry, p, branch, now),
        }
    }

    fn run_routine_agent(&mut self, entry: &Entry, p: usize, branch: String, now: Instant) -> Result<(), String> {
        if !agents::installed(&entry.task.program) {
            return Err(format!("{} is not installed", entry.task.agent));
        }
        let start = control::Start {
            fresh: Some(true),
            ..super::queued_start(entry, (self.projects[p].id, None), Some(branch))
        };
        self.launch_start(entry.id, now, start)
    }

    fn run_routine_flow(&mut self, entry: &Entry, p: usize, name: &str, branch: String) -> Result<(), String> {
        let project = self.projects[p].id;
        let (flow, note) = pick(&self.flow_listing(Some(&self.projects[p].path)), name)?;
        let accounts = self.requested_accounts(None, &entry.task.accounts)?;
        let area = super::super::sized(self.panels.queued.area())?;
        let mut starting = Starting::new(flow, entry.task.prompt.clone(), branch, accounts);
        starting.focus = false;
        starting.fresh = Some(true);
        starting.summary = true;
        starting.note = note;
        let client = self.mark_starting(entry.id, Instant::now())?;
        let answer = self.run_flow(Some(client), project, starting, area);
        self.routine_started(client, entry.id, answer)
    }

    pub(super) fn routine_started(
        &mut self,
        client: u64,
        id: u64,
        answer: super::super::Handled,
    ) -> Result<(), String> {
        match answer {
            Ok(Some(value)) => {
                self.queue_answered(client, &Ok(value));
            }
            Ok(None) => {}
            Err(reason) => return Err(self.fail_starting(client, id, String::from(reason))),
        }
        Ok(())
    }
}
