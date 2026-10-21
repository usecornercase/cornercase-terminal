use std::path::Path;
use std::time::SystemTime;

use serde_json::Value;

use super::{Account, Key, Listed, Merged, Provider, Read, Source};
use crate::error::{Error, Result};
use crate::git::remote::split_url;
use crate::pull_request::glab::{self, Host, Unlisted};
use crate::time::{self, Stamp};

pub(super) const MAX_PAGES: u32 = 20;
const PAGE: usize = 100;
const MISSING: &str = "install glab, GitLab's CLI, to count merged merge requests";
const OLD: &str = "update glab to count merged merge requests";

pub(crate) fn read(glab: &Path, dir: &Path, since: SystemTime) -> Read {
    let hosts = match discover(glab, dir) {
        Ok(hosts) => hosts,
        Err(read) => return read,
    };
    let accounts = hosts
        .into_iter()
        .map(|host| Account {
            listed: list(glab, dir, &host.host, since).map_err(|e| e.to_string()),
            source: Source {
                provider: Provider::Gitlab,
                host: host.host,
                login: host.user,
                ssh_host: Some(host.ssh_host).filter(|h| !h.is_empty()),
                subfolder: host.subfolder,
            },
        })
        .collect();
    Read::Accounts(accounts)
}

fn discover(glab: &Path, dir: &Path) -> std::result::Result<Vec<Host>, Read> {
    let hosts = glab::hosts(glab, dir).map_err(|unlisted| match unlisted {
        Unlisted::Missing => Read::Unavailable(MISSING.into()),
        Unlisted::Old => Read::Unavailable(OLD.into()),
        Unlisted::LoggedOut => Read::Unavailable(glab::LOGGED_OUT.into()),
        Unlisted::Failed(message) => Read::Failed(message),
    })?;
    let hosts: Vec<Host> = hosts.into_iter().filter(Host::signed_in).collect();
    if hosts.is_empty() { Err(Read::Unavailable(glab::LOGGED_OUT.into())) } else { Ok(hosts) }
}

fn list(glab: &Path, dir: &Path, host: &str, since: SystemTime) -> Result<Listed> {
    let mut merged = Vec::new();
    let mut truncated = false;
    for page in 1..=MAX_PAGES {
        let path = format!(
            "merge_requests?scope=created_by_me&state=merged&updated_after={}&order_by=updated_at&sort=desc&per_page={PAGE}&page={page}",
            Stamp(since)
        );
        let nodes = match page_of(glab, dir, host, &path) {
            Ok(nodes) => nodes,
            Err(e) if page == 1 => return Err(e),
            Err(_) => {
                truncated = true;
                break;
            }
        };
        merged.extend(nodes.iter().filter_map(|node| node_of(node, host)));
        if nodes.len() < PAGE {
            break;
        }
        truncated = page == MAX_PAGES;
    }
    merged.retain(|m: &Merged| m.at >= since);
    Ok(Listed { merged, truncated })
}

fn page_of(glab: &Path, dir: &Path, host: &str, path: &str) -> Result<Vec<Value>> {
    let out = glab::run(glab, dir, &["api", "--hostname", host, path])?;
    if !out.status.success() {
        return Err(Error::Glab(glab::said(&out).lines().next().unwrap_or_default().to_string()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| Error::Glab(format!("glab printed something unexpected: {e}")))
}

fn node_of(node: &Value, host: &str) -> Option<Merged> {
    let number = node["iid"].as_u64()?;
    let at = time::parse_time(node["merged_at"].as_str()?).and_then(super::at)?;
    let path = node["references"]["full"]
        .as_str()
        .and_then(|full| full.rsplit_once('!').map(|(path, _)| path.to_string()))
        .or_else(|| {
            let (_, path) = split_url(node["web_url"].as_str()?)?;
            path.split_once("/-/merge_requests/").map(|(path, _)| path.to_string())
        })?;
    Some(Merged { key: Key { host: host.to_lowercase(), path: path.to_lowercase(), number }, at })
}
