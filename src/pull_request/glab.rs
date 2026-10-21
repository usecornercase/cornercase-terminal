use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, Result};
use crate::process;

const TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) const LOGGED_OUT: &str = "glab is not logged in: run glab auth login";

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[expect(clippy::struct_field_names, reason = "the keys glab prints")]
pub(crate) struct Host {
    #[serde(default)]
    pub(crate) host: String,
    #[serde(default)]
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) user: String,
    #[serde(default)]
    pub(crate) ssh_host: String,
    #[serde(default)]
    pub(crate) subfolder: String,
    #[serde(default)]
    pub(crate) error: String,
    #[serde(default)]
    pub(crate) token_source: String,
}

impl Host {
    pub(crate) fn serves(&self, host: &str) -> bool {
        let host = host.to_lowercase();
        let own = self.host.to_lowercase();
        host == own
            || host.ends_with(&format!(".{own}"))
            || (!self.ssh_host.is_empty() && self.ssh_host.to_lowercase() == host)
    }

    pub(crate) fn signed_in(&self) -> bool {
        self.state == "success"
    }

    pub(crate) fn why(&self) -> String {
        if self.token_source == "none" || self.error.is_empty() || unauthorized(&self.error) {
            LOGGED_OUT.into()
        } else {
            self.error.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unlisted {
    Missing,
    Old,
    LoggedOut,
    Failed(String),
}

pub(crate) fn unauthorized(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["401 unauthorized", "http 401", ": 401 "].iter().any(|status| lower.contains(status)) || lower.ends_with(": 401")
}

pub(crate) fn run(glab: &Path, dir: &Path, args: &[&str]) -> Result<Output> {
    let mut command = Command::new(glab);
    command.current_dir(dir).args(args).envs(ENV);
    process::output_within(&mut command, TIMEOUT)
        .map_err(Error::RunGlab)?
        .ok_or_else(|| Error::RunGlab(io::Error::new(io::ErrorKind::TimedOut, "no answer within a minute")))
}

pub(crate) fn hosts(glab: &Path, dir: &Path) -> std::result::Result<Vec<Host>, Unlisted> {
    let out = match run(glab, dir, &["auth", "status", "--all", "--output", "json"]) {
        Ok(out) => out,
        Err(Error::RunGlab(e)) if e.kind() == io::ErrorKind::NotFound => return Err(Unlisted::Missing),
        Err(e) => return Err(Unlisted::Failed(e.to_string())),
    };
    let Ok(status) = serde_json::from_slice::<Status>(&out.stdout) else {
        return Err(unlisted(&said(&out)));
    };
    let mut hosts: Vec<Host> = Vec::new();
    for host in status.hosts.into_iter().filter(|h| !h.host.is_empty()) {
        if !hosts.iter().any(|h| h.host.eq_ignore_ascii_case(&host.host)) {
            hosts.push(host);
        }
    }
    if hosts.is_empty() { Err(Unlisted::LoggedOut) } else { Ok(hosts) }
}

fn unlisted(said: &str) -> Unlisted {
    let lower = said.to_lowercase();
    if lower.contains("unknown flag") || lower.contains("unknown shorthand") {
        Unlisted::Old
    } else if lower.contains("no gitlab instances") || lower.contains("glab auth login") {
        Unlisted::LoggedOut
    } else {
        Unlisted::Failed(said.lines().next().unwrap_or_default().to_string())
    }
}

pub(crate) fn said(out: &Output) -> String {
    if let Some(message) = envelope(&out.stdout) {
        return message;
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty() && *l != "ERROR").collect();
    if !lines.is_empty() {
        return lines.join("\n");
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if stdout.is_empty() { format!("glab failed with {}", out.status) } else { stdout }
}

fn envelope(stdout: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(stdout).ok()?;
    value.get("error")?.as_object()?.get("message")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests;
