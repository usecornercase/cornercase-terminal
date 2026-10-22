use std::io;
use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::gh;

use stack::{Stack, StackMark};

pub(crate) mod gitlab;
pub(crate) mod glab;
pub(crate) mod merged;
pub(crate) mod stack;

const FIELDS: &str = "number,title,url,state,isDraft,mergeable,baseRefName,statusCheckRollup";

const NOT_FOUND: &str = "no pull requests found";

const LOGGED_OUT: &str = "gh auth login";

const NEVER: &str = "0001-01-01";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Provider {
    Github,
    Gitlab,
}

impl Provider {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Gitlab => "GitLab",
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }

    pub(crate) fn of(id: &str) -> Self {
        if id == "gitlab" { Self::Gitlab } else { Self::Github }
    }

    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Github => "pull request",
            Self::Gitlab => "merge request",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Github => "Pull request",
            Self::Gitlab => "Merge request",
        }
    }

    pub(crate) fn checks(self) -> &'static str {
        match self {
            Self::Github => "checks",
            Self::Gitlab => "pipeline",
        }
    }

    pub(crate) fn reference(self, number: u64) -> String {
        match self {
            Self::Github => format!("#{number}"),
            Self::Gitlab => format!("!{number}"),
        }
    }

    pub(crate) fn find(self, tool: &Path, dir: &Path) -> Result<Option<PullRequest>> {
        match self {
            Self::Github => find(tool, dir),
            Self::Gitlab => gitlab::find(tool, dir),
        }
    }

    pub(crate) fn explain(self, error: &Error) -> String {
        match self {
            Self::Github => explain(error),
            Self::Gitlab => gitlab::explain(error),
        }
    }
}

pub(crate) fn asks_glab(host: &str) -> bool {
    let host = host.to_lowercase();
    host != "github.com" && !host.ends_with(".github.com")
}

pub(crate) fn provider_of(host: &str, hosts: &[glab::Host]) -> Provider {
    if host.to_lowercase().contains("gitlab") || hosts.iter().any(|h| h.serves(host)) {
        Provider::Gitlab
    } else {
        Provider::Github
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Open,
    Draft,
    Merged,
    Closed,
}

impl State {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Draft => "draft",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }

    pub(crate) fn settled(self) -> bool {
        matches!(self, Self::Merged | Self::Closed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Outcome {
    Failed,
    Pending,
    Passed,
    Skipped,
}

impl Outcome {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ci {
    Failing,
    Pending,
    Passing,
}

impl Ci {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Failing => "failing",
            Self::Pending => "pending",
            Self::Passing => "passing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) workflow: String,
    pub(crate) outcome: Outcome,
    pub(crate) url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: State,
    pub(crate) conflicting: Option<bool>,
    pub(crate) base: String,
    pub(crate) checks: Vec<Check>,
    pub(crate) provider: Provider,
    pub(crate) stack: Option<Stack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark {
    pub(crate) number: u64,
    pub(crate) state: State,
    pub(crate) ci: Option<Ci>,
    pub(crate) conflicts: bool,
    pub(crate) stack: Option<StackMark>,
}

impl PullRequest {
    pub(crate) fn ci(&self) -> Option<Ci> {
        let has = |outcome: Outcome| self.checks.iter().any(|c| c.outcome == outcome);
        if has(Outcome::Failed) {
            Some(Ci::Failing)
        } else if has(Outcome::Pending) {
            Some(Ci::Pending)
        } else if has(Outcome::Passed) {
            Some(Ci::Passing)
        } else {
            None
        }
    }

    pub(crate) fn running(&self) -> bool {
        !self.state.settled() && self.ci() == Some(Ci::Pending)
    }

    pub(crate) fn mark(&self) -> Mark {
        let stack = self.stack.as_ref().and_then(|s| s.mark(self.number, !self.state.settled()));
        Mark { number: self.number, state: self.state, ci: self.ci(), conflicts: self.conflicts(), stack }
    }

    pub(crate) fn conflicts(&self) -> bool {
        !self.state.settled() && self.conflicting == Some(true)
    }

    pub(crate) fn keep_known(&mut self, before: &Self) {
        if self.number != before.number {
            return;
        }
        if self.conflicting.is_none() {
            self.conflicting = before.conflicting;
        }
        if self.stack.is_none() {
            self.stack.clone_from(&before.stack);
        }
    }

    pub(crate) fn reference(&self) -> String {
        self.provider.reference(self.number)
    }

    pub(crate) fn base_name(&self) -> &str {
        if self.base.is_empty() { "its base" } else { &self.base }
    }

    pub(crate) fn count(&self, outcome: Outcome) -> usize {
        self.checks.iter().filter(|c| c.outcome == outcome).count()
    }

    pub(crate) fn failed(&self) -> impl Iterator<Item = &Check> {
        self.checks.iter().filter(|c| c.outcome == Outcome::Failed)
    }

    pub(crate) fn summary(&self) -> String {
        let counts: Vec<String> = [Outcome::Failed, Outcome::Pending, Outcome::Passed, Outcome::Skipped]
            .into_iter()
            .map(|outcome| (self.count(outcome), outcome.name()))
            .filter(|(n, _)| *n > 0)
            .map(|(n, name)| format!("{n} {name}"))
            .collect();
        let checks = if counts.is_empty() { format!("no {}", self.provider.checks()) } else { counts.join(", ") };
        if self.conflicts() {
            format!("{} · conflicts with {} · {checks}", self.state.name(), self.base_name())
        } else {
            format!("{} · {checks}", self.state.name())
        }
    }

    pub(crate) fn problems(&self) -> Option<String> {
        let checks: Vec<String> = self
            .failed()
            .map(
                |check| if check.url.is_empty() { check.name.clone() } else { format!("{} {}", check.name, check.url) },
            )
            .collect();
        let mut parts = Vec::new();
        if self.conflicts() {
            parts.push(format!("has merge conflicts with {}", self.base_name()));
        }
        if !checks.is_empty() {
            parts.push(format!("failed its {}: {}", self.provider.checks(), checks.join(", ")));
        }
        if parts.is_empty() {
            return None;
        }
        Some(format!("{} {} {} ", self.provider.noun(), self.reference(), parts.join(" and ")))
    }
}

pub(crate) fn find(gh: &Path, dir: &Path) -> Result<Option<PullRequest>> {
    match gh::run::<Raw>(gh, dir, &["pr", "view", "--json", FIELDS]) {
        Ok(raw) => Ok(Some(raw.into())),
        Err(Error::Gh(message)) if message.starts_with(NOT_FOUND) => Ok(None),
        Err(e) => Err(e),
    }
}

pub(crate) fn explain(error: &Error) -> String {
    match error {
        Error::RunGh(e) if e.kind() == io::ErrorKind::NotFound => "install gh, GitHub's CLI, to see them".into(),
        Error::Gh(message) if message.contains(LOGGED_OUT) => "gh is not logged in: run gh auth login".into(),
        error => error.to_string(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    mergeable: Option<String>,
    #[serde(default)]
    base_ref_name: Option<String>,
    #[serde(default)]
    status_check_rollup: Vec<RawCheck>,
}

#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum RawCheck {
    #[serde(rename_all = "camelCase")]
    CheckRun {
        name: Option<String>,
        status: Option<String>,
        conclusion: Option<String>,
        details_url: Option<String>,
        workflow_name: Option<String>,
        started_at: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StatusContext {
        context: Option<String>,
        state: Option<String>,
        target_url: Option<String>,
        started_at: Option<String>,
    },
    #[serde(other)]
    Other,
}

impl RawCheck {
    fn check(self) -> Option<(Check, Option<String>)> {
        match self {
            Self::CheckRun { name, status, conclusion, details_url, workflow_name, started_at } => {
                let completed = status.as_deref() == Some("COMPLETED");
                let outcome = match conclusion.as_deref().filter(|_| completed) {
                    Some("SUCCESS") => Outcome::Passed,
                    Some("NEUTRAL" | "SKIPPED") => Outcome::Skipped,
                    Some("FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE") => {
                        Outcome::Failed
                    }
                    _ => Outcome::Pending,
                };
                let check = Check {
                    name: name.unwrap_or_default(),
                    workflow: workflow_name.unwrap_or_default(),
                    outcome,
                    url: details_url.unwrap_or_default(),
                };
                Some((check, started(started_at)))
            }
            Self::StatusContext { context, state, target_url, started_at } => {
                let outcome = match state.as_deref() {
                    Some("SUCCESS") => Outcome::Passed,
                    Some("FAILURE" | "ERROR") => Outcome::Failed,
                    _ => Outcome::Pending,
                };
                let check = Check {
                    name: context.unwrap_or_default(),
                    workflow: String::new(),
                    outcome,
                    url: target_url.unwrap_or_default(),
                };
                Some((check, started(started_at)))
            }
            Self::Other => None,
        }
    }
}

pub(super) fn state_of(state: &str, draft: bool) -> State {
    match state {
        "MERGED" => State::Merged,
        "CLOSED" => State::Closed,
        _ if draft => State::Draft,
        _ => State::Open,
    }
}

pub(super) fn conflicting_of(mergeable: Option<&str>) -> Option<bool> {
    match mergeable {
        Some("CONFLICTING") => Some(true),
        Some("MERGEABLE") => Some(false),
        _ => None,
    }
}

pub(super) fn order(checks: &mut [Check]) {
    checks.sort_by(|a, b| {
        let key = |c: &Check| (c.outcome, c.workflow.is_empty(), c.workflow.clone(), c.name.clone());
        key(a).cmp(&key(b))
    });
}

fn started(at: Option<String>) -> Option<String> {
    at.filter(|at| !at.is_empty() && !at.starts_with(NEVER))
}

impl From<Raw> for PullRequest {
    fn from(raw: Raw) -> Self {
        let state = state_of(&raw.state, raw.is_draft);
        let mut runs: Vec<(Check, Option<String>)> =
            raw.status_check_rollup.into_iter().filter_map(RawCheck::check).collect();
        let newest_first = |started: &Option<String>| (started.is_none(), started.clone());
        runs.sort_by(|(a, a_started), (b, b_started)| {
            (&a.workflow, &a.name)
                .cmp(&(&b.workflow, &b.name))
                .then_with(|| newest_first(b_started).cmp(&newest_first(a_started)))
        });
        runs.dedup_by(|(later, _), (kept, _)| (&later.workflow, &later.name) == (&kept.workflow, &kept.name));
        let mut checks: Vec<Check> = runs.into_iter().map(|(check, _)| check).collect();
        order(&mut checks);
        let conflicting = conflicting_of(raw.mergeable.as_deref());
        let base = raw.base_ref_name.unwrap_or_default();
        Self {
            number: raw.number,
            title: raw.title,
            url: raw.url,
            state,
            conflicting,
            base,
            checks,
            provider: Provider::Github,
            stack: None,
        }
    }
}

#[cfg(test)]
mod tests;
