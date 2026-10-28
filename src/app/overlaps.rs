use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::App;
use super::changes_panel::ChangesEvent;
use super::event::AppEvent;
use crate::changes::overlap::{self, Known, Pair, Scan};
use crate::control::SharedInfo;
use crate::git;
use crate::log::{Job, Level};
use crate::project::{Hierarchy, Project};

pub(super) const SCAN_EVERY: Duration = Duration::from_secs(5);

const SLOW: Duration = Duration::from_secs(10);

const SHARED_LISTED: usize = 100;

#[derive(Default)]
pub(super) struct Overlaps {
    pub(super) scans: HashMap<u64, Scanned>,
    pub(super) scanning: HashSet<u64>,
}

pub(super) struct Scanned {
    pub(super) done: Instant,
    known: Known,
    pairs: Vec<Pair>,
}

impl Scanned {
    fn files(&self, a: &Path, b: &Path) -> Option<&[String]> {
        self.pairs
            .iter()
            .find(|((first, second), _)| (first == a && second == b) || (first == b && second == a))
            .map(|(_, files)| files.as_slice())
    }
}

fn checkouts(project: &Project) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for workspace in project.workspaces.iter().filter(|w| w.open()) {
        if !dirs.contains(&workspace.path) && git::branch(&workspace.path).is_some() {
            dirs.push(workspace.path.clone());
        }
    }
    dirs
}

impl App {
    pub(super) fn scan_overlaps(&mut self, now: Instant) {
        self.git.overlaps.scans.retain(|id, _| self.projects.project_index(*id).is_some());
        self.close_gone_merge_check();
        if !self.config.shared_files {
            self.git.overlaps.scans.clear();
            return;
        }
        for p in 0..self.projects.len() {
            self.scan_project_overlaps(p, now);
        }
    }

    fn scan_project_overlaps(&mut self, p: usize, now: Instant) {
        let project = &self.projects[p];
        let (id, dirs) = (project.id, checkouts(project));
        if dirs.len() < 2 || project.closing {
            self.git.overlaps.scans.remove(&id);
            return;
        }
        let overlaps = &mut self.git.overlaps;
        let recent = overlaps.scans.get(&id).is_some_and(|s| now.saturating_duration_since(s.done) < SCAN_EVERY);
        if recent || !overlaps.scanning.insert(id) {
            return;
        }
        let known = overlaps.scans.get(&id).map(|s| s.known.clone()).unwrap_or_default();
        let job = Job::new(Level::Debug, "changes", "overlaps").with("project", id).with("checkouts", dirs.len());
        self.spawn_job(
            job.slow_after(SLOW),
            move || Ok(overlap::scan(&dirs, &known)),
            move |scan, _| {
                let scan = Box::new(scan.unwrap_or_default());
                Some(AppEvent::Changes(ChangesEvent::Overlaps { project: id, scan }))
            },
        );
    }

    pub(super) fn overlaps_scanned(&mut self, project: u64, scan: Scan) {
        self.git.overlaps.scanning.remove(&project);
        if self.projects.project_index(project).is_none() {
            return;
        }
        let Scan { pairs, known } = scan;
        self.git.overlaps.scans.insert(project, Scanned { done: Instant::now(), known, pairs });
    }

    pub(super) fn shared_with(&self, workspace: u64) -> Vec<(u64, &[String])> {
        let Some(at) = self.projects.workspace_index(workspace).filter(|_| self.config.shared_files) else {
            return Vec::new();
        };
        let (project, ws) = (&self.projects[at.project()], self.projects.workspace(at));
        let Some(scanned) = self.git.overlaps.scans.get(&project.id).filter(|_| !project.closing && ws.open()) else {
            return Vec::new();
        };
        let others = project.workspaces.iter().filter(|o| o.open() && o.path != ws.path);
        others.filter_map(|o| scanned.files(&ws.path, &o.path).map(|files| (o.id, files))).collect()
    }

    pub(super) fn shared_files(&self, workspace: u64) -> HashSet<&str> {
        self.shared_with(workspace).into_iter().flat_map(|(_, files)| files).map(String::as_str).collect()
    }

    pub(super) fn shared_count(&self, workspace: u64) -> usize {
        self.shared_files(workspace).len()
    }

    pub(super) fn shared_info(&self, workspace: u64) -> Vec<SharedInfo> {
        let info = |(other, files): (u64, &[String])| SharedInfo {
            workspace: other,
            count: files.len(),
            files: files.iter().take(SHARED_LISTED).cloned().collect(),
        };
        self.shared_with(workspace).into_iter().map(info).collect()
    }
}
