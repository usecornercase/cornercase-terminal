use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{CLOSED_EVERY, Checkout, Mode, git};
use crate::error::{Error, Result};

const SHARE: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) base: String,
    pub(crate) files: usize,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    asked: Option<String>,
    digest: u64,
}

pub(crate) struct Target {
    pub(crate) checkout: Checkout,
    pub(crate) working: bool,
}

#[derive(Debug)]
struct Loading {
    generation: u64,
    workspaces: Vec<u64>,
    base: Option<String>,
    at: Instant,
}

enum Outcome {
    Counted(Tally),
    Same { base: Option<String>, digest: u64 },
    Keep,
    Drop,
}

#[derive(Debug, Default)]
pub(crate) struct Tallies {
    held: HashMap<u64, Tally>,
    asked: HashMap<u64, Instant>,
    wait: HashMap<u64, Duration>,
    loading: Option<Loading>,
    generation: u64,
}

impl Tallies {
    pub(crate) fn busy(&self) -> bool {
        self.loading.is_some()
    }

    pub(crate) fn get(&self, workspace: u64, base: Option<&str>) -> Option<&Tally> {
        self.held.get(&workspace).filter(|t| t.asked.as_deref() == base)
    }

    fn due(&self, target: &Target, now: Instant) -> bool {
        let workspace = target.checkout.workspace;
        let Some(asked) = self.asked.get(&workspace) else { return true };
        let held = self.held.get(&workspace);
        if held.is_some_and(|t| t.asked != target.checkout.base) {
            return true;
        }
        let wait = self.wait.get(&workspace).copied().unwrap_or(CLOSED_EVERY);
        now.duration_since(*asked) >= wait
    }

    pub(crate) fn request(&mut self, targets: &[Target], now: Instant) -> Option<(u64, u64, git::Request)> {
        if self.busy() {
            return None;
        }
        let first = targets
            .iter()
            .filter(|t| self.due(t, now))
            .min_by_key(|t| (!t.working, self.asked.get(&t.checkout.workspace).copied()))?;
        let (dir, base) = (&first.checkout.dir, &first.checkout.base);
        let workspaces: Vec<u64> = targets
            .iter()
            .filter(|t| &t.checkout.dir == dir && &t.checkout.base == base)
            .map(|t| t.checkout.workspace)
            .collect();
        for workspace in &workspaces {
            self.asked.insert(*workspace, now);
        }
        let last = self.shared_digest(&workspaces, base.as_deref());
        let request = git::Request {
            dir: dir.clone(),
            mode: Mode::All,
            base: base.clone(),
            last,
            previous: Vec::new(),
            expanded: Some(Vec::new()),
        };
        self.generation += 1;
        self.loading = Some(Loading { generation: self.generation, workspaces, base: base.clone(), at: now });
        Some((first.checkout.workspace, self.generation, request))
    }

    fn shared_digest(&self, workspaces: &[u64], base: Option<&str>) -> Option<u64> {
        let mut digests = workspaces.iter().map(|w| self.get(*w, base).map(|t| t.digest));
        let first = digests.next()??;
        digests.all(|d| d == Some(first)).then_some(first)
    }

    pub(crate) fn loaded(&mut self, generation: u64, result: Result<git::Loaded>, now: Instant) {
        let Some(loading) = self.loading.take_if(|l| l.generation == generation) else { return };
        let took = now.saturating_duration_since(loading.at);
        let wait = CLOSED_EVERY.max(took.saturating_mul(SHARE));
        let outcome = outcome_of(result, loading.base.as_ref());
        for workspace in loading.workspaces {
            self.wait.insert(workspace, wait);
            self.apply(workspace, &outcome);
        }
    }

    fn apply(&mut self, workspace: u64, outcome: &Outcome) {
        match outcome {
            Outcome::Counted(tally) => drop(self.held.insert(workspace, tally.clone())),
            Outcome::Same { base, digest } => self.refresh(workspace, base.as_deref(), *digest),
            Outcome::Keep => {}
            Outcome::Drop => drop(self.held.remove(&workspace)),
        }
    }

    fn refresh(&mut self, workspace: u64, base: Option<&str>, digest: u64) {
        let Some(held) = self.held.get_mut(&workspace) else { return };
        held.digest = digest;
        if let Some(base) = base {
            base.clone_into(&mut held.base);
        }
    }

    pub(crate) fn forget(&mut self, workspace: u64) {
        self.held.remove(&workspace);
        self.asked.remove(&workspace);
        self.wait.remove(&workspace);
        if let Some(loading) = &mut self.loading {
            loading.workspaces.retain(|w| *w != workspace);
        }
    }

    pub(crate) fn rescan(&mut self, workspace: u64) {
        self.asked.remove(&workspace);
        self.wait.remove(&workspace);
    }
}

fn outcome_of(result: Result<git::Loaded>, asked: Option<&String>) -> Outcome {
    match result {
        Ok(git::Loaded { base, digest, diff: None }) => Outcome::Same { base, digest },
        Ok(git::Loaded { base, digest, diff: Some(diff) }) => Outcome::Counted(Tally {
            base: base.or_else(|| asked.cloned()).unwrap_or_default(),
            files: diff.files.len(),
            added: diff.added(),
            removed: diff.removed(),
            asked: asked.cloned(),
            digest,
        }),
        Err(Error::Git(message)) if message == "timed out" => Outcome::Keep,
        Err(_) => Outcome::Drop,
    }
}

#[cfg(test)]
mod tests;
