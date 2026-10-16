use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use serde_json::Value;

use super::{Account, Key, Listed, Merged, Provider, Read, Source};
use crate::error::{Error, Result};
use crate::gh;
use crate::git::remote::split_url;
use crate::process;
use crate::time::{self, Stamp};

const TIMEOUT: Duration = Duration::from_secs(60);
const MISSING: &str = "install glab, GitLab's CLI, to count merged merge requests";
const LOGGED_OUT: &str = "glab is not logged in: run glab auth login";
const OLD: &str = "update glab to count merged merge requests";

const ENV: [(&str, &str); 6] = [
    ("GLAB_NO_PROMPT", "true"),
    ("GLAB_CHECK_UPDATE", "false"),
    ("GLAB_SEND_TELEMETRY", "false"),
    ("GLAB_SHOW_WHATS_NEW", "false"),
    ("GLAB_NOTIFY_SKILL_UPDATES", "false"),
    ("NO_COLOR", "1"),
];

#[derive(Deserialize)]
struct Status {
    #[serde(default)]
    hosts: Vec<Host>,
}

#[derive(Deserialize)]
#[expect(clippy::struct_field_names, reason = "the keys glab prints")]
struct Host {
    #[serde(default)]
    host: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    ssh_host: String,
    #[serde(default)]
    subfolder: String,
}

fn run(glab: &Path, dir: &Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new(glab);
    command.current_dir(dir).args(args).envs(ENV);
    process::output_within(&mut command, TIMEOUT)
        .map_err(Error::RunGlab)?
        .ok_or_else(|| Error::RunGlab(io::Error::new(io::ErrorKind::TimedOut, "no answer within a minute")))
}

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
    let out = match run(glab, dir, &["auth", "status", "--all", "--output", "json"]) {
        Ok(out) => out,
        Err(Error::RunGlab(e)) if e.kind() == io::ErrorKind::NotFound => return Err(Read::Unavailable(MISSING.into())),
        Err(e) => return Err(Read::Failed(e.to_string())),
    };
    let Ok(status) = serde_json::from_slice::<Status>(&out.stdout) else {
        let message = gh::failure(&out, "glab");
        return Err(if message.contains("unknown flag") || message.contains("unknown shorthand") {
            Read::Unavailable(OLD.into())
        } else if message.contains("no GitLab instances") || message.contains("glab auth login") {
            Read::Unavailable(LOGGED_OUT.into())
        } else {
            Read::Failed(message)
        });
    };
    let mut hosts: Vec<Host> = Vec::new();
    for host in status.hosts.into_iter().filter(|h| h.state == "success" && !h.host.is_empty()) {
        if !hosts.iter().any(|h| h.host.eq_ignore_ascii_case(&host.host)) {
            hosts.push(host);
        }
    }
    if hosts.is_empty() { Err(Read::Unavailable(LOGGED_OUT.into())) } else { Ok(hosts) }
}

fn list(glab: &Path, dir: &Path, host: &str, since: SystemTime) -> Result<Listed> {
    let path = format!(
        "merge_requests?scope=created_by_me&state=merged&updated_after={}&order_by=updated_at&sort=desc&per_page=100",
        Stamp(since)
    );
    let out = run(glab, dir, &["api", "--hostname", host, "--paginate", &path])?;
    if !out.status.success() {
        return Err(Error::Glab(gh::failure(&out, "glab")));
    }
    let mut merged = Vec::new();
    for page in serde_json::Deserializer::from_slice(&out.stdout).into_iter::<Value>() {
        let page = page.map_err(|e| Error::Glab(format!("glab printed something unexpected: {e}")))?;
        merged.extend(page.as_array().into_iter().flatten().filter_map(|node| node_of(node, host)));
    }
    merged.retain(|m: &Merged| m.at >= since);
    Ok(Listed { merged, truncated: false })
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
