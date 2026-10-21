use std::io;
use std::path::Path;

use serde::Deserialize;

use super::{Check, Outcome, Provider, PullRequest, State, glab, order};
use crate::error::{Error, Result};

const VIEW: [&str; 4] = ["mr", "view", "--output", "json"];

const NONE: &str = "no open merge request available";

const SEVERAL: &str = "possible matches:";

const MISSING: &str = "install glab, GitLab's CLI, to see them";

const UNKNOWN: [&str; 3] = ["checking", "unchecked", "preparing"];

const PIPELINE: &str = "pipeline";

pub(crate) fn find(glab: &Path, dir: &Path) -> Result<Option<PullRequest>> {
    let Some(raw) = view(glab, dir)? else { return Ok(None) };
    let checks = match &raw.head_pipeline {
        Some(pipeline) => pipeline_checks(glab, dir, pipeline)?,
        None => Vec::new(),
    };
    Ok(Some(raw.into_pull(checks)))
}

fn view(glab: &Path, dir: &Path) -> Result<Option<Raw>> {
    let out = glab::run(glab, dir, &VIEW)?;
    if out.status.success() {
        return parse(&out.stdout).map(Some);
    }
    let said = glab::said(&out);
    let lower = said.to_lowercase();
    if lower.contains(NONE) {
        return Ok(None);
    }
    if lower.contains(SEVERAL)
        && let Some(number) = newest(&said)
    {
        return view_number(glab, dir, number).map(Some);
    }
    Err(Error::Glab(first_line(&said)))
}

fn first_line(said: &str) -> String {
    said.lines().next().unwrap_or_default().to_string()
}

fn newest(said: &str) -> Option<u64> {
    said.lines()
        .map(str::trim)
        .skip_while(|line| !line.to_lowercase().contains(SEVERAL))
        .find_map(|line| line.strip_prefix('!'))
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
}

fn view_number(glab: &Path, dir: &Path, number: u64) -> Result<Raw> {
    let number = number.to_string();
    let out = glab::run(glab, dir, &["mr", "view", &number, "--output", "json"])?;
    if !out.status.success() {
        return Err(Error::Glab(first_line(&glab::said(&out))));
    }
    parse(&out.stdout)
}

fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|e| Error::Glab(format!("glab printed something unexpected: {e}")))
}

pub(crate) fn explain(error: &Error) -> String {
    match error {
        Error::RunGlab(e) if e.kind() == io::ErrorKind::NotFound => MISSING.into(),
        Error::Glab(message) if message.contains("glab auth login") || message.contains("401") => {
            glab::LOGGED_OUT.into()
        }
        error => error.to_string(),
    }
}

fn pipeline_checks(glab: &Path, dir: &Path, pipeline: &Pipeline) -> Result<Vec<Check>> {
    let mut checks = match (pipeline.id, pipeline.project_id) {
        (Some(id), Some(project)) => jobs(glab, dir, project, id)?,
        _ => Vec::new(),
    };
    if let Some(check) = pipeline.check(&checks) {
        checks.push(check);
    }
    Ok(checks)
}

fn jobs(glab: &Path, dir: &Path, project: u64, id: u64) -> Result<Vec<Check>> {
    let path = format!("projects/{project}/pipelines/{id}/jobs?per_page=100");
    let out = glab::run(glab, dir, &["api", "--paginate", &path])?;
    if !out.status.success() {
        return Err(Error::Glab(first_line(&glab::said(&out))));
    }
    let pages = serde_json::Deserializer::from_slice(&out.stdout).into_iter::<Vec<Job>>();
    let mut checks = Vec::new();
    for page in pages {
        let page = page.map_err(|e| Error::Glab(format!("glab printed something unexpected: {e}")))?;
        checks.extend(page.into_iter().map(Job::check));
    }
    Ok(checks)
}

fn outcome(status: &str, allow_failure: bool) -> Outcome {
    match status {
        "success" => Outcome::Passed,
        "failed" if allow_failure => Outcome::Skipped,
        "failed" | "canceled" => Outcome::Failed,
        "skipped" | "manual" => Outcome::Skipped,
        _ => Outcome::Pending,
    }
}

#[derive(Deserialize)]
struct Raw {
    iid: u64,
    title: Option<String>,
    web_url: Option<String>,
    state: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    work_in_progress: bool,
    has_conflicts: Option<bool>,
    detailed_merge_status: Option<String>,
    target_branch: Option<String>,
    head_pipeline: Option<Pipeline>,
}

#[derive(Deserialize)]
struct Pipeline {
    id: Option<u64>,
    project_id: Option<u64>,
    status: Option<String>,
    web_url: Option<String>,
}

#[derive(Deserialize)]
struct Job {
    name: Option<String>,
    stage: Option<String>,
    status: Option<String>,
    web_url: Option<String>,
    #[serde(default)]
    allow_failure: bool,
}

impl Job {
    fn check(self) -> Check {
        Check {
            name: self.name.unwrap_or_default(),
            workflow: self.stage.unwrap_or_default(),
            outcome: outcome(self.status.as_deref().unwrap_or_default(), self.allow_failure),
            url: self.web_url.unwrap_or_default(),
        }
    }
}

impl Pipeline {
    fn check(&self, jobs: &[Check]) -> Option<Check> {
        let outcome = outcome(self.status.as_deref().unwrap_or_default(), false);
        let any = |wanted: Outcome| jobs.iter().any(|c| c.outcome == wanted);
        let worse = match outcome {
            Outcome::Failed => !any(Outcome::Failed),
            Outcome::Pending => !any(Outcome::Failed) && !any(Outcome::Pending),
            Outcome::Passed | Outcome::Skipped => false,
        };
        worse.then(|| Check {
            name: PIPELINE.into(),
            workflow: String::new(),
            outcome,
            url: self.web_url.clone().unwrap_or_default(),
        })
    }
}

impl Raw {
    fn state(&self) -> State {
        match self.state.as_deref() {
            Some("merged") => State::Merged,
            Some("closed") => State::Closed,
            _ if self.draft || self.work_in_progress => State::Draft,
            _ => State::Open,
        }
    }

    fn conflicting(&self) -> Option<bool> {
        match self.detailed_merge_status.as_deref() {
            Some(status) if UNKNOWN.contains(&status) => None,
            Some("conflict") => Some(true),
            _ => self.has_conflicts,
        }
    }

    fn into_pull(self, mut checks: Vec<Check>) -> PullRequest {
        order(&mut checks);
        PullRequest {
            number: self.iid,
            state: self.state(),
            conflicting: self.conflicting(),
            title: self.title.unwrap_or_default(),
            url: self.web_url.unwrap_or_default(),
            base: self.target_branch.unwrap_or_default(),
            checks,
            provider: Provider::Gitlab,
        }
    }
}

#[cfg(test)]
mod tests;
