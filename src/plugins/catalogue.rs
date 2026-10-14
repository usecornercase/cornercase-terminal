use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use serde::Deserialize;

use super::manifest::{self, Manifest};
use super::registry::Entry as Installed;
use super::source::Origin;
use crate::error::{Error, Result};
use crate::http::{self, Service};
use crate::version;

pub(crate) const URL: &str = "https://usecornercase.dev/plugins/index.json";
pub(crate) const URL_ENV: &str = "CORNERCASE_PLUGIN_INDEX";
pub(crate) const GITHUB: &str = "https://github.com/";
pub(crate) const VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 1024 * 1024;
const MAX_PLUGINS: usize = 500;
const MAX_PERMISSIONS: usize = 16;
const TIMEOUT: Duration = Duration::from_secs(15);
const TARGETS: [&str; 4] =
    ["aarch64-apple-darwin", "x86_64-apple-darwin", "aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"];
const PLATFORMS: [&str; 2] = ["macos", "linux"];
const SERVICE: Service = Service { name: "the plugin catalogue", rejected: "the plugin catalogue refused the request" };

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Index {
    pub(crate) version: u32,
    pub(crate) plugins: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) repo: String,
    #[serde(default, rename = "ref")]
    pub(crate) reference: String,
    pub(crate) commit: String,
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) min_cornercase: Option<String>,
    #[serde(default)]
    pub(crate) platforms: Vec<String>,
    #[serde(default)]
    pub(crate) permissions: Vec<String>,
    #[serde(default)]
    pub(crate) sha256: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mark {
    Available,
    Installed(String),
    Unfit(String),
}

pub(crate) fn url() -> String {
    std::env::var(URL_ENV).ok().filter(|url| !url.trim().is_empty()).unwrap_or_else(|| URL.into())
}

pub(crate) fn fetch(url: &str) -> Result<Index> {
    let bytes = http::download_within(&SERVICE, url, MAX_BYTES, TIMEOUT)?;
    parse(&bytes).map_err(Error::Plugin)
}

pub(crate) fn parse(bytes: &[u8]) -> std::result::Result<Index, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(format!("the plugin catalogue is larger than {} KiB", MAX_BYTES / 1024));
    }
    let head: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| format!("the plugin catalogue: {e}"))?;
    match head.get("version").and_then(serde_json::Value::as_u64) {
        Some(v) if v == u64::from(VERSION) => {}
        Some(v) => return Err(format!("the plugin catalogue is version {v}; this cornercase reads {VERSION}")),
        None => return Err("the plugin catalogue has no version".into()),
    }
    let index: Index = serde_json::from_value(head).map_err(|e| format!("the plugin catalogue: {e}"))?;
    if index.plugins.len() > MAX_PLUGINS {
        return Err(format!("the plugin catalogue lists more than {MAX_PLUGINS} plugins"));
    }
    let mut seen = HashSet::new();
    for entry in &index.plugins {
        entry.check().map_err(|e| format!("the plugin catalogue, `{}`: {e}", clean(&entry.id)))?;
        if !seen.insert(entry.id.as_str()) {
            return Err(format!("the plugin catalogue lists `{}` twice", entry.id));
        }
    }
    Ok(index)
}

pub(crate) fn clean(text: &str) -> String {
    text.chars().map(|c| if unsafe_char(c) { ' ' } else { c }).collect()
}

fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

fn text(field: &str, value: &str, max: usize, empty: bool) -> std::result::Result<(), String> {
    if !empty && value.trim().is_empty() {
        return Err(format!("`{field}` is empty"));
    }
    if value.chars().count() > max {
        return Err(format!("`{field}` is longer than {max} characters"));
    }
    if value.chars().any(unsafe_char) {
        return Err(format!("`{field}` holds control characters"));
    }
    Ok(())
}

fn hex(field: &str, value: &str, len: usize) -> std::result::Result<(), String> {
    if value.len() == len && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Ok(());
    }
    Err(format!("`{field}` must be {len} lower-case hexadecimal digits"))
}

pub(crate) fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else { return false };
    let owner_ok = (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let name_ok = (1..=100).contains(&name.len())
        && !name.starts_with(['-', '.'])
        && std::path::Path::new(name).extension().is_none_or(|e| !e.eq_ignore_ascii_case("git"))
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    owner_ok && name_ok
}

fn valid_ref(reference: &str) -> bool {
    reference.is_empty()
        || (reference.len() <= 100
            && !reference.starts_with(['-', '/'])
            && !reference.contains("..")
            && reference.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b)))
}

impl Entry {
    fn check(&self) -> std::result::Result<(), String> {
        if !manifest::is_id(&self.id) {
            return Err("`id` must be lower-case letters, digits, `.`, `_` or `-`, at most 64 characters".into());
        }
        text("name", &self.name, 64, false)?;
        text("description", &self.description, 200, false)?;
        text("version", &self.version, 32, false)?;
        if !valid_repo(&self.repo) {
            return Err("`repo` must be `owner/repo` on github.com".into());
        }
        if !valid_ref(&self.reference) {
            return Err("`ref` must be a tag or a branch name".into());
        }
        hex("commit", &self.commit, 40)?;
        if let Some(min) = &self.min_cornercase {
            text("min_cornercase", min, 32, false)?;
        }
        if let Some(platform) = self.platforms.iter().find(|p| !PLATFORMS.contains(&p.as_str())) {
            return Err(format!("`platforms` holds `{}`; known ones are macos and linux", clean(platform)));
        }
        if self.permissions.len() > MAX_PERMISSIONS {
            return Err(format!("more than {MAX_PERMISSIONS} `permissions`"));
        }
        for permission in &self.permissions {
            text("permissions", permission, 32, false)?;
        }
        for (target, sum) in &self.sha256 {
            if !TARGETS.contains(&target.as_str()) {
                return Err(format!("`sha256` names `{}`, which is not a target cornercase builds", clean(target)));
            }
            hex("sha256", sum, 64)?;
        }
        Ok(())
    }

    pub(crate) fn origin(&self, github: &str) -> Origin {
        let url = if github.starts_with('/') {
            format!("{github}/{}", self.repo)
        } else {
            format!("{github}{}.git", self.repo)
        };
        Origin { repo: self.repo.clone(), url, reference: Some(self.commit.clone()) }
    }

    pub(crate) fn matches(&self, commit: &str, manifest: &Manifest) -> std::result::Result<(), String> {
        if commit != self.commit {
            return Err(format!("{} gave commit {commit}, not the reviewed {}", self.repo, self.commit));
        }
        if manifest.id != self.id {
            return Err(format!("{} holds the plugin `{}`, not `{}`", self.repo, clean(&manifest.id), self.id));
        }
        for (target, sum) in &self.sha256 {
            if manifest.download.get(target).is_none_or(|download| !download.sha256.eq_ignore_ascii_case(sum)) {
                return Err(format!("its download for {target} is not the one reviewed"));
            }
        }
        Ok(())
    }

    pub(crate) fn mark(&self, installed: &[Installed]) -> Mark {
        if let Some(entry) = installed.iter().find(|entry| entry.id == self.id) {
            return Mark::Installed(entry.source.describe());
        }
        if !self.platforms.is_empty() && !self.platforms.iter().any(|p| p == manifest::platform()) {
            return Mark::Unfit(format!("runs on {} only", self.platforms.join(", ")));
        }
        if let Some(min) = self.min_cornercase.as_ref().filter(|min| version::newer(min, version::CURRENT)) {
            return Mark::Unfit(format!("needs cornercase {min} or later"));
        }
        Mark::Available
    }

    pub(crate) fn asks(&self) -> String {
        Manifest::asking(&self.permissions)
    }
}

#[cfg(test)]
mod tests;
