use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use super::App;
use super::event::AppEvent;
use crate::agents::stats::MergedRow;
use crate::git::remote::{self, Remote};
use crate::log::{self, Job, Level};
use crate::pull_request::merged::{self, Answer, Cache, Provider, Read};

const EVERY: Duration = Duration::from_mins(15);

const FAILED_EVERY: Duration = Duration::from_mins(5);

pub(super) const STALE: Duration = Duration::from_mins(5);

const SLOW: Duration = Duration::from_secs(30);

const NOTE: &str = "your merged pull requests, from gh and glab";

#[derive(Default)]
pub(super) struct MergedPulls {
    cache: Cache,
    remotes: Vec<Remote>,
    hosts: BTreeSet<String>,
    asking: bool,
    asked: Option<Instant>,
    failed: bool,
    reasons: BTreeSet<String>,
}

impl MergedPulls {
    #[cfg(test)]
    pub(super) fn asking(&self) -> bool {
        self.asking
    }

    pub(super) fn row(&self, since: SystemTime, now: SystemTime) -> Option<MergedRow> {
        self.cache.count(&self.remotes, since, now)
    }
}

impl App {
    pub(super) fn merged_note(&self) -> String {
        let (github, gitlab) =
            (self.merged.cache.sources(Provider::Github), self.merged.cache.sources(Provider::Gitlab));
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let mut parts = Vec::new();
        if github > 0 {
            parts.push(format!("{github} GitHub account{}", plural(github)));
        }
        if gitlab > 0 {
            parts.push(format!("{gitlab} GitLab account{}", plural(gitlab)));
        }
        if !parts.is_empty() {
            return format!("from {}", parts.join(" and "));
        }
        let note = self.merged.cache.note();
        if note.is_empty() { NOTE.into() } else { note }
    }

    pub(super) fn ask_merged(&mut self, now: Instant) {
        self.ask_merged_within(now, None);
    }

    pub(super) fn ask_merged_stale(&mut self, now: Instant) {
        self.ask_merged_within(now, Some(STALE));
    }

    fn ask_merged_within(&mut self, now: Instant, stale: Option<Duration>) {
        if !self.config.merged_pull_requests {
            self.merged = MergedPulls::default();
            return;
        }
        let mut remotes: Vec<Remote> = Vec::new();
        for found in self.projects.iter().filter(|p| !p.closing).flat_map(|project| remote::of(&project.path)) {
            if !remotes.contains(&found) {
                remotes.push(found);
            }
        }
        let hosts: BTreeSet<String> = remotes.iter().map(|r| r.host.clone()).collect();
        let changed = hosts != self.merged.hosts;
        self.merged.remotes = remotes;
        if self.merged.remotes.is_empty() || self.merged.asking {
            return;
        }
        let every = stale.unwrap_or(if self.merged.failed { FAILED_EVERY } else { EVERY });
        let due = self.merged.asked.is_none_or(|at| now.saturating_duration_since(at) >= every);
        if !(due || (changed && stale.is_none())) {
            return;
        }
        self.merged.hosts = hosts;
        self.merged.asking = true;
        self.merged.asked = Some(now);
        self.spawn_merged();
    }

    fn spawn_merged(&self) {
        let gh = self.config.gh(self.home.as_deref());
        let glab = self.config.glab(self.home.as_deref());
        let dir = self.home.clone().unwrap_or_else(|| PathBuf::from("/"));
        let since = self.agents.stats.since();
        let job = Job::new(Level::Debug, "pr", "merged").slow_after(SLOW);
        self.spawn_job(
            job,
            move || Ok(merged::read(&gh, &glab, &dir, since, SystemTime::now())),
            move |result, _| {
                let answer = result.unwrap_or_else(|e| Answer {
                    github: Read::Failed(e.to_string()),
                    gitlab: Read::Failed(e.to_string()),
                });
                Some(AppEvent::Merged(answer))
            },
        );
    }

    pub(super) fn merged_answered(&mut self, answer: Answer) {
        self.merged.asking = false;
        self.merged.asked = Some(Instant::now());
        if !self.config.merged_pull_requests {
            return;
        }
        self.merged.cache.apply(answer);
        let problems = self.merged.cache.problems();
        let reasons: BTreeSet<String> =
            problems.iter().map(|(p, host, why)| format!("{}\u{0}{host}\u{0}{why}", p.name())).collect();
        for (provider, host, why) in &problems {
            let key = format!("{}\u{0}{host}\u{0}{why}", provider.name());
            if !self.merged.reasons.contains(&key) {
                log::warning!(
                    "pr",
                    "cannot read merged pull requests",
                    provider = provider.name(),
                    host = host,
                    error = why
                );
            }
        }
        let failed = !problems.is_empty();
        if self.merged.failed && !failed {
            log::info!("pr", "merged pull requests read again");
        }
        self.merged.failed = failed;
        self.merged.reasons = reasons;
        log::info!(
            "pr",
            "merged pull requests read",
            github = self.merged.cache.sources(Provider::Github),
            gitlab = self.merged.cache.sources(Provider::Gitlab)
        );
    }
}
