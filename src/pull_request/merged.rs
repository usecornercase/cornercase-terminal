use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::agents::stats::{self, MergedAccount, MergedRow};
use crate::git::remote::Remote;

pub(crate) mod github;
pub(crate) mod gitlab;

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

    fn index(self) -> usize {
        match self {
            Self::Github => 0,
            Self::Gitlab => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) provider: Provider,
    pub(crate) host: String,
    pub(crate) login: String,
    pub(crate) ssh_host: Option<String>,
    pub(crate) subfolder: String,
}

impl Source {
    fn id(&self) -> (Provider, String, String) {
        (self.provider, self.host.to_lowercase(), self.login.to_lowercase())
    }

    fn serves(&self, host: &str) -> bool {
        let host = host.to_lowercase();
        let own = self.host.to_lowercase();
        host == own
            || host.ends_with(&format!(".{own}"))
            || self.ssh_host.as_ref().is_some_and(|ssh| ssh.to_lowercase() == host)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub(crate) host: String,
    pub(crate) path: String,
    pub(crate) number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Merged {
    pub(crate) key: Key,
    pub(crate) at: SystemTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) merged: Vec<Merged>,
    pub(crate) truncated: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Account {
    pub(crate) source: Source,
    pub(crate) listed: Result<Listed, String>,
}

#[derive(Debug, Clone)]
pub(crate) enum Read {
    Unavailable(String),
    Failed(String),
    Accounts(Vec<Account>),
}

#[derive(Debug, Clone)]
pub(crate) struct Answer {
    pub(crate) github: Read,
    pub(crate) gitlab: Read,
}

pub(crate) fn read(gh: &Path, glab: &Path, dir: &Path, since: SystemTime, now: SystemTime) -> Answer {
    Answer { github: github::read(gh, dir, since, now), gitlab: gitlab::read(glab, dir, since) }
}

pub(crate) fn at(seconds: i64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_secs(u64::try_from(seconds).ok()?))
}

#[derive(Debug, Clone)]
struct Entry {
    source: Source,
    last: Option<Listed>,
    error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Cache {
    entries: Vec<Entry>,
    unavailable: [Option<String>; 2],
    failed: [Option<String>; 2],
}

impl Cache {
    pub(crate) fn apply(&mut self, answer: Answer) {
        self.apply_read(Provider::Github, answer.github);
        self.apply_read(Provider::Gitlab, answer.gitlab);
    }

    fn apply_read(&mut self, provider: Provider, read: Read) {
        let slot = provider.index();
        match read {
            Read::Unavailable(why) => {
                self.entries.retain(|e| e.source.provider != provider);
                self.unavailable[slot] = Some(why);
                self.failed[slot] = None;
            }
            Read::Failed(why) => {
                for entry in self.entries.iter_mut().filter(|e| e.source.provider == provider) {
                    entry.error = Some(why.clone());
                }
                self.failed[slot] = Some(why);
                self.unavailable[slot] = None;
            }
            Read::Accounts(accounts) => {
                self.unavailable[slot] = None;
                self.failed[slot] = None;
                self.merge_accounts(provider, accounts);
            }
        }
    }

    fn merge_accounts(&mut self, provider: Provider, accounts: Vec<Account>) {
        let mut old: HashMap<_, _> =
            self.entries.iter().filter(|e| e.source.provider == provider).map(|e| (e.source.id(), e.clone())).collect();
        self.entries.retain(|e| e.source.provider != provider);
        for account in accounts {
            if self.entries.iter().any(|e| e.source.id() == account.source.id()) {
                continue;
            }
            let entry = old.remove(&account.source.id());
            self.entries.push(refreshed(entry, account));
        }
    }

    pub(crate) fn accounts(&self) -> Vec<MergedAccount> {
        self.entries
            .iter()
            .map(|e| MergedAccount {
                provider: e.source.provider.name().to_string(),
                host: e.source.host.clone(),
                login: e.source.login.clone(),
                error: e.error.clone(),
            })
            .collect()
    }

    pub(crate) fn problems(&self) -> Vec<(Provider, String, String)> {
        let mut found: Vec<(Provider, String, String)> = Vec::new();
        for provider in [Provider::Github, Provider::Gitlab] {
            if let Some(why) = &self.failed[provider.index()] {
                found.push((provider, String::new(), why.clone()));
            }
        }
        found.extend(
            self.entries
                .iter()
                .filter_map(|e| e.error.as_ref().map(|why| (e.source.provider, e.source.host.clone(), why.clone()))),
        );
        found
    }

    pub(crate) fn note(&self) -> String {
        if let Some((_, _, why)) = self.problems().first() {
            return why.clone();
        }
        match (&self.unavailable[0], &self.unavailable[1]) {
            (Some(_), Some(_)) => "install and log in to gh or glab to count them".into(),
            (Some(why), None) | (None, Some(why)) => why.clone(),
            (None, None) => String::new(),
        }
    }

    pub(crate) fn sources(&self, provider: Provider) -> usize {
        self.entries.iter().filter(|e| e.source.provider == provider).count()
    }

    pub(crate) fn count(&self, remotes: &[Remote], since: SystemTime, now: SystemTime) -> Option<MergedRow> {
        if remotes.is_empty() || self.entries.iter().all(|e| e.last.is_none()) {
            return None;
        }
        let scope = Scope::of(remotes, &self.entries);
        let found: HashMap<&Key, SystemTime> = self
            .entries
            .iter()
            .filter_map(|e| e.last.as_ref())
            .flat_map(|listed| &listed.merged)
            .filter(|merged| scope.has(&merged.key))
            .map(|merged| (&merged.key, merged.at))
            .collect();
        let [day, week, total] = stats::windows(found.values().copied(), since, now);
        let complete = self.entries.iter().all(|e| e.error.is_none() && e.last.as_ref().is_some_and(|l| !l.truncated))
            && self.failed.iter().all(Option::is_none);
        Some(MergedRow { day, week, total, complete, accounts: self.accounts() })
    }
}

fn refreshed(old: Option<Entry>, account: Account) -> Entry {
    let mut entry = old.unwrap_or(Entry { source: account.source.clone(), last: None, error: None });
    entry.source = account.source;
    match account.listed {
        Ok(listed) => {
            entry.last = Some(listed);
            entry.error = None;
        }
        Err(why) => entry.error = Some(why),
    }
    entry
}

struct Scope {
    placed: BTreeSet<(String, String)>,
    unplaced: BTreeSet<String>,
}

impl Scope {
    fn of(remotes: &[Remote], entries: &[Entry]) -> Self {
        let mut placed = BTreeSet::new();
        let mut unplaced = BTreeSet::new();
        for remote in remotes {
            let mut matched = false;
            for entry in entries.iter().filter(|e| e.source.serves(&remote.host)) {
                matched = true;
                let prefix = format!("{}/", entry.source.subfolder.trim_matches('/'));
                let path = match remote.path.strip_prefix(&prefix) {
                    Some(rest) if remote.web && prefix != "/" => rest,
                    _ => remote.path.as_str(),
                };
                placed.insert((entry.source.host.to_lowercase(), path.to_lowercase()));
            }
            let host = remote.host.to_lowercase();
            let alias = !host.contains('.') || entries.iter().any(|e| host.starts_with(&e.source.host.to_lowercase()));
            if !matched && alias {
                unplaced.insert(remote.path.to_lowercase());
            }
        }
        Self { placed, unplaced }
    }

    fn has(&self, key: &Key) -> bool {
        self.placed.contains(&(key.host.clone(), key.path.clone())) || self.unplaced.contains(&key.path)
    }
}

#[cfg(test)]
mod tests;
