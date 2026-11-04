use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Remote;
use crate::{paths, persist};

const LONGEST: usize = 255;
const OPTION: &str = "--command";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Machine {
    pub(crate) destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
}

impl Machine {
    pub(crate) fn of(remote: &Remote) -> Self {
        Self { destination: remote.destination.clone(), command: remote.command.clone() }
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
    let machines: Vec<Machine> = serde_json::from_slice(&bytes).unwrap_or_default();
    machines.into_iter().filter(|machine| valid(&machine.destination)).collect()
}

pub(crate) fn remember(file: &Path, machine: Machine) {
    let mut machines = read(file);
    match machines.iter_mut().find(|known| known.destination == machine.destination) {
        Some(known) if *known == machine => return,
        Some(known) => *known = machine,
        None => machines.push(machine),
    }
    let _ = persist::write(file, &machines, true);
}

pub(crate) fn forget(file: &Path, destination: &str) {
    let mut machines = read(file);
    let before = machines.len();
    machines.retain(|machine| machine.destination != destination);
    if machines.len() != before {
        let _ = persist::write(file, &machines, true);
    }
}

pub(crate) fn valid(destination: &str) -> bool {
    !destination.is_empty()
        && destination.len() <= LONGEST
        && !destination.starts_with('-')
        && !destination.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests;
