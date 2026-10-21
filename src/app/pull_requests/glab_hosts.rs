use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use super::SLOW;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::worktrees::WorktreeEvent;
use crate::git;
use crate::log::{Job, Level};
use crate::pull_request::glab::{self, Host, Unlisted};
use crate::pull_request::{Provider, asks_glab, provider_of};

const HOSTS_EVERY: Duration = Duration::from_mins(15);

const FAILED_EVERY: Duration = Duration::from_mins(5);

const MISSING: &str = "install glab, GitLab's CLI, to see them";

const OLD: &str = "update glab to see them";

pub(in crate::app) type Listed = Result<Vec<Host>, String>;

#[derive(Default)]
pub(in crate::app) struct GlabHosts {
    listed: Option<Listed>,
    asking: bool,
    asked: Option<Instant>,
    wanted: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Route {
    Wait,
    Ask(Provider),
    Explain(String),
}

impl GlabHosts {
    #[cfg(test)]
    pub(in crate::app) fn asking(&self) -> bool {
        self.asking
    }

    pub(super) fn route(&self, host: Option<&str>) -> Route {
        let Some(host) = host.filter(|h| asks_glab(h)) else { return Route::Ask(Provider::Github) };
        let Some(listed) = &self.listed else { return Route::Wait };
        let hosts = listed.as_ref().map_or(&[][..], Vec::as_slice);
        if provider_of(host, hosts) == Provider::Github {
            return Route::Ask(Provider::Github);
        }
        match listed {
            Err(why) => Route::Explain(why.clone()),
            Ok(hosts) => {
                let serving = || hosts.iter().filter(|h| h.serves(host));
                match serving().find(|h| h.signed_in()).or_else(|| serving().next()) {
                    Some(h) if h.signed_in() => Route::Ask(Provider::Gitlab),
                    Some(h) => Route::Explain(h.why()),
                    None => Route::Explain(glab::LOGGED_OUT.into()),
                }
            }
        }
    }

    fn signed_in(&self) -> bool {
        matches!(&self.listed, Some(Ok(hosts)) if hosts.iter().any(Host::signed_in))
    }

    fn due(&self, now: Instant, wanted: &BTreeSet<String>) -> bool {
        let every = if self.signed_in() { HOSTS_EVERY } else { FAILED_EVERY };
        !wanted.is_subset(&self.wanted) || self.asked.is_none_or(|at| now.saturating_duration_since(at) >= every)
    }
}

impl App {
    pub(super) fn project_host(&self, p: usize) -> Option<String> {
        git::remote::primary(&self.projects[p].path).map(|r| r.host)
    }

    pub(super) fn glab_wanted(&self) -> BTreeSet<String> {
        (0..self.projects.len())
            .filter(|p| !self.projects[*p].closing)
            .filter_map(|p| self.project_host(p))
            .filter(|host| asks_glab(host))
            .collect()
    }

    pub(super) fn ask_glab_hosts(&mut self, now: Instant, wanted: BTreeSet<String>) {
        let hosts = &mut self.git.pulls.glab;
        if wanted.is_empty() || hosts.asking || !hosts.due(now, &wanted) {
            return;
        }
        hosts.asking = true;
        hosts.wanted = wanted;
        let glab = self.config.glab(self.home.as_deref());
        let dir = self.home.clone().unwrap_or_else(|| "/".into());
        self.spawn_job(
            Job::new(Level::Debug, "pr", "glab hosts").slow_after(SLOW),
            move || Ok(glab::hosts(&glab, &dir).map_err(unlisted)),
            move |result, _| {
                let listed = result.unwrap_or_else(|e| Err(e.to_string()));
                Some(AppEvent::Worktree(WorktreeEvent::GlabHosts(listed)))
            },
        );
    }

    pub(in crate::app) fn glab_hosts_found(&mut self, listed: Listed) {
        let hosts = &mut self.git.pulls.glab;
        hosts.asking = false;
        if !self.config.pull_requests {
            return;
        }
        hosts.asked = Some(Instant::now());
        if hosts.listed.as_ref() != Some(&listed) {
            self.git.pulls.answered.retain(|_, a| a.provider != Provider::Gitlab);
        }
        self.git.pulls.glab.listed = Some(listed);
    }
}

fn unlisted(why: Unlisted) -> String {
    match why {
        Unlisted::Missing => MISSING.into(),
        Unlisted::Old => OLD.into(),
        Unlisted::LoggedOut => glab::LOGGED_OUT.into(),
        Unlisted::Failed(message) => message,
    }
}

#[cfg(test)]
mod tests;
