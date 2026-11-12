use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::machines;
use crate::{paths, persist};

const LIMIT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Kept {
    pub(crate) port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) local: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) pid: Option<i32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) closed: bool,
}

impl Kept {
    pub(crate) fn new(port: u16) -> Self {
        Self { port, local: None, pid: None, closed: false }
    }
}

type All = BTreeMap<String, Vec<Kept>>;

pub(crate) fn file() -> PathBuf {
    paths::tunnels(&paths::session())
}

fn read_all(file: &Path) -> All {
    let Ok(bytes) = std::fs::read(file) else { return All::new() };
    let Ok(Value::Object(destinations)) = serde_json::from_slice::<Value>(&bytes) else { return All::new() };
    destinations
        .into_iter()
        .filter(|(destination, _)| machines::valid(destination))
        .filter_map(|(destination, entries)| match entries {
            Value::Array(entries) => Some((destination, entries_of(entries))),
            _ => None,
        })
        .collect()
}

fn entries_of(entries: Vec<Value>) -> Vec<Kept> {
    let mut kept: Vec<Kept> = Vec::new();
    for entry in entries {
        let Ok(entry) = serde_json::from_value::<Kept>(entry) else { continue };
        if entry.port != 0 && kept.iter().all(|known| known.port != entry.port) {
            kept.push(entry);
        }
    }
    kept.sort_by_key(|entry| entry.port);
    kept.truncate(LIMIT);
    kept
}

pub(crate) fn read(file: &Path, destination: &str) -> Vec<Kept> {
    read_all(file).remove(destination).unwrap_or_default()
}

pub(crate) fn update(
    file: &Path,
    destination: &str,
    port: u16,
    change: impl FnOnce(Option<Kept>) -> Option<Kept>,
) -> io::Result<bool> {
    let mut all = read_all(file);
    let entries = all.entry(destination.to_string()).or_default();
    let before = entries.clone();
    let old = entries.iter().find(|entry| entry.port == port).copied();
    entries.retain(|entry| entry.port != port);
    entries.extend(change(old));
    entries.sort_by_key(|entry| entry.port);
    entries.truncate(LIMIT);
    if *entries == before {
        return Ok(false);
    }
    all.retain(|_, entries| !entries.is_empty());
    persist::write(file, &all, true)?;
    Ok(true)
}

pub(crate) fn forget(file: &Path, destination: &str, port: u16) -> io::Result<bool> {
    update(file, destination, port, |_| None)
}

#[cfg(test)]
mod tests;
