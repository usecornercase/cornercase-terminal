use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::overlay::Overlay;
use crate::app::worktrees::{Measure, WorktreeEvent};
use crate::error::Result;
use crate::log::{Job, Level};
use crate::text;
use crate::worktrees::builds::{self, Size};
use crate::worktrees::{Kind, Row, SIZE_GIVE_UP, SIZES_AT_ONCE};

#[derive(Default)]
pub(in crate::app) struct Sizes {
    queue: VecDeque<Measuring>,
    running: HashMap<u64, (PathBuf, Instant)>,
}

struct Measuring {
    path: PathBuf,
    kind: Kind,
    names: Vec<String>,
}

impl App {
    pub(super) fn builds_of(&self, repo: &Path) -> Vec<String> {
        self.config.project(repo, self.home.as_deref()).builds()
    }

    pub(super) fn queue_sizes(&mut self, rows: &[Row]) {
        let queued: Vec<Measuring> = rows
            .iter()
            .filter(|row| row.has_folder())
            .map(|row| Measuring {
                path: row.found.path.clone(),
                kind: row.found.kind,
                names: self.builds_of(&row.found.repo),
            })
            .collect();
        self.git.scans.sizes.queue.extend(queued);
    }

    pub(super) fn pump_sizes(&mut self) {
        while self.git.scans.sizes.running.len() < SIZES_AT_ONCE {
            let Some(next) = self.git.scans.sizes.queue.pop_front() else { break };
            self.spawn_size(next);
        }
    }

    fn spawn_size(&mut self, Measuring { path, kind, names }: Measuring) {
        let generation = self.git.scans.generation;
        let ticket = self.git.scans.next;
        self.git.scans.next += 1;
        self.git.scans.sizes.running.insert(ticket, (path.clone(), Instant::now()));
        let job = Job::new(Level::Debug, "worktrees", "size").with("path", path.display());
        let (at, answer) = (path.clone(), Measure::Dialog { generation, ticket });
        self.spawn_job(
            job,
            move || builds::measure(&at, kind, &names),
            move |size, job| {
                if let Ok(size) = &size {
                    job.note("bytes", size.bytes);
                }
                Some(AppEvent::Worktree(WorktreeEvent::Measured { answer, path, size: Box::new(size) }))
            },
        );
    }

    pub(in crate::app) fn worktree_measured(&mut self, answer: Measure, path: &Path, size: Result<Size>) {
        let (generation, ticket) = match answer {
            Measure::Dialog { generation, ticket } => (generation, ticket),
            Measure::Request(key) => return self.worktree_size_answered(key, path.to_path_buf(), size),
        };
        if self.git.scans.sizes.running.remove(&ticket).is_none() {
            return;
        }
        if self.worktrees_dialog(generation).is_some() {
            self.apply_size(path, size.map_err(|e| e.to_string()));
            self.pump_sizes();
        }
    }

    pub(super) fn apply_size(&mut self, path: &Path, size: std::result::Result<Size, String>) {
        if let Some(Overlay::Worktrees(dialog)) = &mut self.overlay {
            dialog.list.sized(path, size);
        }
    }

    pub(super) fn give_up_sizes(&mut self, now: Instant) {
        let late: Vec<u64> = self
            .git
            .scans
            .sizes
            .running
            .iter()
            .filter(|(_, (_, since))| now.saturating_duration_since(*since) >= SIZE_GIVE_UP)
            .map(|(ticket, _)| *ticket)
            .collect();
        for ticket in late {
            if let Some((path, _)) = self.git.scans.sizes.running.remove(&ticket) {
                crate::log::warning!("worktrees", "gave up on a size", path = path.display());
                self.apply_size(&path, Err("gave up".to_string()));
            }
        }
        self.pump_sizes();
    }

    pub(super) fn sizes_note(dialog: &crate::worktrees::Dialog) -> Option<String> {
        let waiting = dialog.list.sizes_pending();
        let total = dialog.list.rows.iter().filter(|row| row.has_folder() && !row.gone).count();
        (waiting > 0).then(|| format!("measuring {} of {total}…", total - waiting))
    }

    pub(super) fn total_text(dialog: &crate::worktrees::Dialog, now: SystemTime) -> Option<(String, String)> {
        let (bytes, count) = dialog.list.total();
        if count == 0 {
            return None;
        }
        let more = if dialog.list.sizes_pending() > 0 { "+" } else { "" };
        let all = text::size(bytes);
        let (idle, _) = dialog.list.idle_builds(now);
        let mut long = format!("{all}{more} in {}", text::plural(count, "worktree", "worktrees"));
        let mut short = format!("{all}{more}");
        if idle > 0 {
            let idle = text::size(idle);
            long = format!("{long} · {idle} of builds idle 7+ days");
            short = format!("{short} · {idle} idle");
        }
        Some((long, short))
    }
}
