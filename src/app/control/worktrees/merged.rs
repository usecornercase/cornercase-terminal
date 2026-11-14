use std::path::{Path, PathBuf};

use super::{Freeing, Risky};
use crate::app::App;
use crate::app::control::{Asker, Handled, json};
use crate::app::event::AppEvent;
use crate::app::worktrees::{ListFor, WorktreeEvent};
use crate::control::contract::{Code, Refusal};
use crate::control::worktrees::merged::{FAILED, FREEABLE, FREED, REFUSED};
use crate::control::{BuildsFreed, FreeMergedBuilds, MergedBuildsFreed, WorktreeFreed};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::worktrees::{Found, Row};

type Target = std::result::Result<Freeing, Refusal>;

impl App {
    pub(in crate::app) fn free_merged_builds_request(&mut self, client: u64, request: &FreeMergedBuilds) -> Handled {
        let keep = self.worktrees_repo(request.project)?;
        let key = self.clients.requests.key();
        self.spawn_worktree_listing(ListFor::FreeMerged { key, check: request.check }, keep);
        self.pend_reading(client, key);
        Ok(None)
    }

    pub(in crate::app) fn merged_rows_listed(&mut self, key: u64, check: bool, found: Result<Vec<Found>>) {
        let event = move |rows| WorktreeEvent::MergedRows { key, check, rows };
        self.rows_listed(key, false, found, event);
    }

    pub(in crate::app) fn merged_rows_ready(&mut self, key: u64, check: bool, rows: Result<Vec<Row>>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        if pending.client.is_none() {
            return;
        }
        let mut rows = match rows {
            Ok(rows) => rows,
            Err(e) => return self.refuse_or_answer(pending.client, Err(Refusal::new(Code::Failed, e.to_string()))),
        };
        self.relive_rows(&mut rows);
        let (targets, keys) = self.merged_targets(&rows, check);
        let job = Job::new(Level::Info, "control", "free merged builds")
            .with("worktrees", targets.len())
            .with("check", check);
        self.spawn_job(
            job,
            move || Ok(merged_answer(targets, check)),
            move |result, _| {
                let result = Box::new(result);
                Some(AppEvent::Worktree(WorktreeEvent::MergedFreed { key, keys, result }))
            },
        );
        self.clients.requests.pending.push(pending);
    }

    fn merged_targets(&mut self, rows: &[Row], check: bool) -> (Vec<(PathBuf, Target)>, Vec<u64>) {
        let mut keys = Vec::new();
        let mut targets = Vec::new();
        for row in rows.iter().filter(|row| row.merged_to_free()) {
            let path = row.found.path.clone();
            let target = self.freeing(Asker::default(), None, Some(&path), check);
            if let (Ok(freeing), false) = (&target, check) {
                let key = self.clients.requests.key();
                self.git.frees.request_frees(key, freeing.path.clone());
                keys.push(key);
            }
            targets.push((path, target));
        }
        (targets, keys)
    }

    pub(in crate::app) fn merged_freed(&mut self, key: u64, keys: &[u64], result: Result<MergedBuildsFreed>) {
        for ended in keys {
            self.git.frees.request_ended(*ended);
        }
        self.answer_read(key, |_| {
            result.map(|answer| json(&answer)).map_err(|e| Refusal::new(Code::Failed, e.to_string()))
        });
    }
}

fn merged_answer(targets: Vec<(PathBuf, Target)>, check: bool) -> MergedBuildsFreed {
    let worktrees: Vec<WorktreeFreed> =
        targets.into_iter().filter_map(|(path, target)| worktree_freed(&path, target, check)).collect();
    MergedBuildsFreed { check, bytes: worktrees.iter().map(|entry| entry.freed.bytes).sum(), worktrees }
}

fn entry(outcome: &str, why: Option<String>, freed: BuildsFreed) -> WorktreeFreed {
    WorktreeFreed { outcome: outcome.to_string(), why, freed }
}

fn worktree_freed(path: &Path, target: Target, check: bool) -> Option<WorktreeFreed> {
    let bare = |path: &Path| BuildsFreed { path: path.to_path_buf(), check, ..BuildsFreed::default() };
    let freeing = match target {
        Ok(freeing) => freeing,
        Err(refusal) => return Some(entry(REFUSED, Some(refusal.message), bare(path))),
    };
    match freeing.settle(check) {
        Ok(Ok(freed)) => freed_entry(freed, check),
        Ok(Err(risky)) => kept_entry(path, &risky, check),
        Err(e) => Some(entry(FAILED, Some(e.to_string()), bare(path))),
    }
}

fn freed_entry(freed: BuildsFreed, check: bool) -> Option<WorktreeFreed> {
    if freed.folders.is_empty() && freed.failed.is_empty() && freed.kept.is_empty() {
        return None;
    }
    Some(entry(if check { FREEABLE } else { FREED }, None, freed))
}

fn kept_entry(path: &Path, risky: &Risky, check: bool) -> Option<WorktreeFreed> {
    if risky.size.builds_bytes() == 0 {
        return None;
    }
    let freed =
        BuildsFreed { path: path.to_path_buf(), check, running: risky.running.clone(), ..BuildsFreed::default() };
    Some(entry(REFUSED, Some(risky.why(path)), freed))
}

#[cfg(test)]
mod tests;
