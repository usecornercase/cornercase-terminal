use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;

use super::{Account, Key, Listed, Merged, Provider, Read, Source};
use crate::error::Error;
use crate::gh;
use crate::time::{self, Stamp};

const LIMIT: u64 = 1000;
const DEPTH: u32 = 6;
const LOGGED_OUT: &str = "gh is not logged in: run gh auth login";
const MISSING: &str = "install gh, GitHub's CLI, to count merged pull requests";
const OLD: &str = "update gh to count merged pull requests";

const QUERY: &str = "query($q: String!, $endCursor: String) { search(type: ISSUE, query: $q, first: 100, after: $endCursor) { \
    issueCount pageInfo { hasNextPage endCursor } nodes { ... on PullRequest { number mergedAt repository { nameWithOwner } } } } }";

#[derive(Deserialize)]
struct Status {
    #[serde(default)]
    hosts: BTreeMap<String, Vec<Entry>>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    active: bool,
    #[serde(default)]
    login: String,
    #[serde(default)]
    state: String,
}

#[expect(clippy::struct_field_names, reason = "the keys gh prints")]
struct Login {
    host: String,
    login: String,
    active: bool,
}

pub(crate) fn read(gh_path: &Path, dir: &Path, since: SystemTime, now: SystemTime) -> Read {
    let found = match discover(gh_path, dir) {
        Ok(found) => found,
        Err(read) => return read,
    };
    let accounts = found
        .into_iter()
        .map(|account| Account {
            source: Source {
                provider: Provider::Github,
                host: account.host.clone(),
                login: account.login.clone(),
                ssh_host: None,
                subfolder: String::new(),
            },
            listed: list(gh_path, dir, &account, since, now),
        })
        .collect();
    Read::Accounts(accounts)
}

fn discover(gh_path: &Path, dir: &Path) -> Result<Vec<Login>, Read> {
    let out = match gh::run_raw(gh_path, dir, &["auth", "status", "--json", "hosts"]) {
        Ok(out) => out,
        Err(Error::RunGh(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Read::Unavailable(MISSING.into()));
        }
        Err(Error::Gh(message)) if message.contains("unknown flag") => return Err(Read::Unavailable(OLD.into())),
        Err(Error::Gh(message)) if message.contains("not logged in") => {
            return Err(Read::Unavailable(LOGGED_OUT.into()));
        }
        Err(e) => return Err(Read::Failed(e.to_string())),
    };
    let status: Status = serde_json::from_slice(&out).map_err(|e| Read::Failed(Error::GhOutput(e).to_string()))?;
    let mut accounts: Vec<Login> = Vec::new();
    for (host, entries) in status.hosts {
        for entry in entries.into_iter().filter(|e| e.state == "success" && !e.login.is_empty()) {
            let seen = accounts
                .iter()
                .any(|a| a.host.eq_ignore_ascii_case(&host) && a.login.eq_ignore_ascii_case(&entry.login));
            if !seen {
                accounts.push(Login { host: host.clone(), login: entry.login, active: entry.active });
            }
        }
    }
    if accounts.is_empty() {
        return Err(Read::Unavailable(LOGGED_OUT.into()));
    }
    Ok(accounts)
}

fn list(gh_path: &Path, dir: &Path, account: &Login, since: SystemTime, now: SystemTime) -> Result<Listed, String> {
    let token = if account.active {
        None
    } else {
        let out = gh::run_raw(gh_path, dir, &["auth", "token", "--hostname", &account.host, "--user", &account.login])
            .map_err(|e| e.to_string())?;
        let token = String::from_utf8_lossy(&out).trim().to_string();
        (!token.is_empty()).then_some(token)
    };
    let env: Vec<(&str, &str)> =
        token.iter().flat_map(|t| [("GH_TOKEN", t.as_str()), ("GH_ENTERPRISE_TOKEN", t.as_str())]).collect();
    let day =
        |at: SystemTime| at.duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs() / 86_400).unwrap_or(0));
    let mut listed = Listed::default();
    let search = Search { gh_path, dir, env: &env, account, since };
    search.fetch((day(since), day(now)), 0, &mut listed).map_err(|e| match &token {
        Some(token) => e.replace(token.as_str(), "…"),
        None => e,
    })?;
    Ok(listed)
}

struct Search<'a> {
    gh_path: &'a Path,
    dir: &'a Path,
    env: &'a [(&'a str, &'a str)],
    account: &'a Login,
    since: SystemTime,
}

impl Search<'_> {
    fn fetch(&self, (from, to): (i64, i64), depth: u32, listed: &mut Listed) -> Result<(), String> {
        let (merged, count) = self.page_through(from, to)?;
        if count > LIMIT && from < to && depth < DEPTH {
            let middle = from + (to - from) / 2;
            self.fetch((from, middle), depth + 1, listed)?;
            return self.fetch((middle + 1, to), depth + 1, listed);
        }
        listed.truncated |= count > LIMIT;
        listed.merged.extend(merged.into_iter().filter(|m: &Merged| m.at >= self.since));
        Ok(())
    }

    fn page_through(&self, from: i64, to: i64) -> Result<(Vec<Merged>, u64), String> {
        let date = |days: i64| {
            Stamp(UNIX_EPOCH + std::time::Duration::from_hours(days.unsigned_abs() * 24)).to_string()[..10].to_string()
        };
        let account = self.account;
        let search = format!("is:pr is:merged author:{} merged:{}..{}", account.login, date(from), date(to));
        let query = format!("query={QUERY}");
        let q = format!("q={search}");
        let out = gh::run_raw_env(
            self.gh_path,
            self.dir,
            &["api", "graphql", "--hostname", &account.host, "--paginate", "-f", &query, "-f", &q],
            self.env,
        )
        .map_err(|e| e.to_string())?;
        let mut merged = Vec::new();
        let mut count = 0;
        for page in serde_json::Deserializer::from_slice(&out).into_iter::<Value>() {
            let page = page.map_err(|e| Error::GhOutput(e).to_string())?;
            if let Some(errors) = page.get("errors").and_then(Value::as_array).filter(|e| !e.is_empty()) {
                let message = errors[0]["message"].as_str().unwrap_or("GitHub refused the search");
                return Err(message.to_string());
            }
            let search = &page["data"]["search"];
            count = count.max(search["issueCount"].as_u64().unwrap_or(0));
            for node in search["nodes"].as_array().into_iter().flatten() {
                merged.extend(node_of(node, &account.host));
            }
        }
        Ok((merged, count))
    }
}

fn node_of(node: &Value, host: &str) -> Option<Merged> {
    let number = node["number"].as_u64()?;
    let at = time::parse_time(node["mergedAt"].as_str()?).and_then(super::at)?;
    let path = node["repository"]["nameWithOwner"].as_str()?.to_lowercase();
    Some(Merged { key: Key { host: host.to_lowercase(), path, number }, at })
}
