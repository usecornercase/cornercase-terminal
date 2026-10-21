use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::git;

mod dialog;
pub(crate) mod discover;
pub(crate) mod remove;
pub(crate) mod scan;

pub(crate) use dialog::{Action, Dialog, Hit, Step};

pub(crate) const SCANS_AT_ONCE: usize = 4;

pub(crate) const SCAN_GIVE_UP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Linked,
    Prunable,
    Orphan,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) repo: PathBuf,
    pub(crate) project: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) admin: Option<String>,
    pub(crate) kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Scan {
    pub(crate) last_changed: Option<SystemTime>,
    pub(crate) changed: usize,
    pub(crate) unpushed: usize,
    pub(crate) upstream_gone: bool,
    pub(crate) merged: bool,
    pub(crate) has_upstream: bool,
    pub(crate) branch: Option<String>,
    pub(crate) lock: Option<git::worktree::Lock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Live {
    pub(crate) project: u64,
    pub(crate) workspace: u64,
    pub(crate) running: Vec<String>,
    pub(crate) removing: bool,
    pub(crate) role_of_run: bool,
    pub(crate) run_roles: usize,
    pub(crate) pull_merged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) found: Found,
    pub(crate) live: Option<Live>,
    pub(crate) scan: Option<Result<Scan, String>>,
    pub(crate) outside: Vec<String>,
    pub(crate) elsewhere: Vec<String>,
    pub(crate) ticked: bool,
    pub(crate) removing: bool,
    pub(crate) gone: bool,
}

impl Row {
    pub(crate) fn merged(&self) -> bool {
        self.pull_merged() || matches!(&self.scan, Some(Ok(scan)) if scan.merged && scan.has_upstream)
    }

    pub(crate) fn no_commits(&self) -> bool {
        !self.pull_merged() && matches!(&self.scan, Some(Ok(scan)) if scan.merged && !scan.has_upstream)
    }

    fn pull_merged(&self) -> bool {
        self.live.as_ref().is_some_and(|l| l.pull_merged)
    }

    pub(crate) fn branch(&self) -> Option<&str> {
        match &self.scan {
            Some(Ok(scan)) => scan.branch.as_deref(),
            _ => self.found.branch.as_deref(),
        }
    }

    pub(crate) fn checked_on_confirm(&self) -> bool {
        match self.found.kind {
            Kind::Linked => true,
            Kind::Prunable => self.found.branch.is_some(),
            Kind::Orphan | Kind::Unknown => false,
        }
    }

    pub(crate) fn tickable(&self) -> bool {
        !self.gone && !self.removing && !self.live.as_ref().is_some_and(|l| l.removing || l.role_of_run)
    }

    pub(crate) fn risky(&self) -> bool {
        let scan_risk = match &self.scan {
            Some(Ok(scan)) => scan.changed > 0 || scan.lock.is_some(),
            _ => true,
        };
        scan_risk
            || !self.outside.is_empty()
            || !self.elsewhere.is_empty()
            || self.live.as_ref().is_some_and(|l| !l.running.is_empty())
    }

    pub(crate) fn band(&self) -> u8 {
        if self.found.kind != Kind::Linked {
            0
        } else if self.merged() {
            1
        } else {
            2
        }
    }

    fn last_changed(&self) -> Option<SystemTime> {
        match &self.scan {
            Some(Ok(scan)) => scan.last_changed,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct List {
    pub(crate) rows: Vec<Row>,
    pub(crate) sorted: bool,
    pub(crate) filter: String,
}

impl List {
    pub(crate) fn new(mut rows: Vec<Row>) -> Self {
        rows.sort_by(|a, b| (&a.found.project, &a.found.branch).cmp(&(&b.found.project, &b.found.branch)));
        let mut list = Self { rows, sorted: false, filter: String::new() };
        list.sort_when_done();
        list
    }

    pub(crate) fn scanned(&mut self, path: &Path, scan: Result<Scan, String>) {
        self.set_scan(path, scan);
        self.sort_when_done();
    }

    fn set_scan(&mut self, path: &Path, scan: Result<Scan, String>) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.found.path == path) {
            row.scan = Some(scan);
        }
    }

    pub(crate) fn pending(&self) -> usize {
        self.rows.iter().filter(|r| r.found.kind == Kind::Linked && r.scan.is_none()).count()
    }

    pub(crate) fn sort_when_done(&mut self) {
        if self.sorted || self.pending() > 0 {
            return;
        }
        self.sorted = true;
        self.rows.sort_by(|a, b| {
            let (ta, tb) = (a.last_changed(), b.last_changed());
            a.band().cmp(&b.band()).then_with(|| match (ta, tb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
        });
    }

    pub(crate) fn shown(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        (0..self.rows.len())
            .filter(|&i| {
                let row = &self.rows[i];
                !row.gone
                    && (needle.is_empty()
                        || row.found.project.to_lowercase().contains(&needle)
                        || row.found.branch.as_ref().is_some_and(|b| b.to_lowercase().contains(&needle)))
            })
            .collect()
    }

    pub(crate) fn ticked(&self) -> Vec<usize> {
        (0..self.rows.len()).filter(|&i| self.rows[i].ticked).collect()
    }
}

#[cfg(test)]
mod tests;
