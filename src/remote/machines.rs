use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Remote;
use crate::{paths, persist};

const LONGEST: usize = 255;
const OPTION: &str = "--command";
pub(crate) const NOT_A_DESTINATION: &str = "one word that does not start with -, such as devbox or me@devbox";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Machine {
    pub(crate) destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) unreached: bool,
}

impl Machine {
    pub(crate) fn of(remote: &Remote) -> Self {
        Self { destination: remote.destination.clone(), command: remote.command.clone(), unreached: false }
    }

    pub(crate) fn never_reached(destination: String, command: Option<String>) -> Self {
        Self { destination, command, unreached: true }
    }

    pub(crate) fn remote(self) -> Remote {
        Remote { destination: self.destination, command: self.command, option: OPTION }
    }
}

pub(crate) fn file() -> PathBuf {
    paths::machines(&paths::session())
}

pub(crate) fn read(file: &Path) -> Vec<Machine> {
    let Ok(bytes) = std::fs::read(file) else { return Vec::new() };
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap_or_default();
    entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<Machine>(entry).ok())
        .filter(|machine| valid(&machine.destination))
        .collect()
}

pub(crate) fn remember(file: &Path, machine: Machine) -> io::Result<Machine> {
    let mut machines = read(file);
    let Some(known) = machines.iter_mut().find(|known| known.destination == machine.destination) else {
        machines.push(machine.clone());
        persist::write(file, &machines, true)?;
        return Ok(machine);
    };
    let merged = Machine { unreached: known.unreached && machine.unreached, ..machine };
    if *known != merged {
        *known = merged.clone();
        persist::write(file, &machines, true)?;
    }
    Ok(merged)
}

pub(crate) fn forget(file: &Path, destination: &str) -> io::Result<bool> {
    let mut machines = read(file);
    let before = machines.len();
    machines.retain(|machine| machine.destination != destination);
    if machines.len() == before {
        return Ok(false);
    }
    persist::write(file, &machines, true)?;
    Ok(true)
}

pub(crate) fn valid(destination: &str) -> bool {
    !destination.is_empty()
        && destination.len() <= LONGEST
        && !destination.starts_with('-')
        && !destination.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests;
