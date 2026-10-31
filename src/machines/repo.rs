use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;

use crate::git::{self, remote::Repo};
use crate::process;
use crate::text;

const SSH: &str = "ssh";
const WITHIN: Duration = Duration::from_secs(5);

pub(crate) type Resolve = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Hostnames {
    resolve: Resolve,
    known: Arc<Mutex<HashMap<String, Arc<OnceLock<String>>>>>,
}

impl Hostnames {
    pub(crate) fn new(resolve: Resolve) -> Self {
        Self { resolve, known: Arc::default() }
    }

    pub(crate) fn of(&self, host: &str) -> String {
        let once = Arc::clone(self.known.lock().entry(host.to_string()).or_default());
        once.get_or_init(|| (self.resolve)(host).unwrap_or_else(|| host.to_string())).clone()
    }

    pub(crate) fn repo(&self, dir: &Path) -> Repo {
        let remotes: Vec<(String, String)> = git::remote_urls(dir)
            .into_iter()
            .map(|(name, url)| (name, git::remote::without_credentials(&url)))
            .collect();
        let identity = git::remote::identity(&remotes, |host| Some(self.of(host)));
        Repo { remotes, identity }
    }
}

pub(crate) fn pairs(theirs: &[Option<&str>], ours: &[(u64, Option<&str>)]) -> Vec<Option<u64>> {
    theirs
        .iter()
        .map(|repo| {
            let repo = (*repo)?;
            ours.iter().find(|(_, identity)| identity.is_some_and(|ours| ours.eq_ignore_ascii_case(repo))).map(|p| p.0)
        })
        .collect()
}

pub(crate) fn ssh() -> Resolve {
    Arc::new(|host| hostname(SSH, host))
}

fn hostname(program: &str, host: &str) -> Option<String> {
    if host.starts_with('-') || host.chars().any(|c| c.is_whitespace() || text::hidden(c)) {
        return None;
    }
    let output = process::output_within(Command::new(program).args(["-G", "--", host]), WITHIN).ok()??;
    output.status.success().then(|| configured(&String::from_utf8_lossy(&output.stdout)))?
}

fn configured(config: &str) -> Option<String> {
    config
        .lines()
        .find_map(|line| line.strip_prefix("hostname "))
        .map(str::trim)
        .filter(|hostname| !hostname.is_empty())
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests;
