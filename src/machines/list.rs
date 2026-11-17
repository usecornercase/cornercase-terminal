use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ICONS, Machine, name_taken, valid_command, valid_destination};
use crate::remote::Remote;
use crate::remote::machines::valid;
use crate::{config, paths, persist};

const NAME_MAX: usize = 20;
const FALLBACK_NAME: &str = "machine";
const HOME: &str = "~";

pub(crate) struct Here {
    pub(crate) config: PathBuf,
    pub(crate) legacy: PathBuf,
}

impl Here {
    pub(crate) fn this_computer() -> Self {
        Self { config: paths::config(), legacy: paths::machines(&paths::session()) }
    }
}

#[derive(Serialize)]
struct OnlyMachines<'a> {
    machines: &'a [Machine],
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Legacy {
    destination: String,
    command: Option<String>,
    unreached: bool,
}

pub(crate) fn read(here: &Here) -> Vec<Machine> {
    let _ = migrate(here);
    let Ok(bytes) = std::fs::read(&here.config) else { return Vec::new() };
    let Ok(Value::Object(mut keys)) = serde_json::from_slice::<Value>(&bytes) else { return Vec::new() };
    let Some(Value::Array(entries)) = keys.remove("machines") else { return Vec::new() };
    entries.into_iter().filter_map(|entry| serde_json::from_value(entry).ok()).collect()
}

pub(crate) fn change<R>(here: &Here, edit: impl FnOnce(&mut Vec<Machine>) -> R) -> io::Result<R> {
    let _ = migrate(here);
    edit_config(&here.config, edit)
}

fn edit_config<R>(path: &Path, edit: impl FnOnce(&mut Vec<Machine>) -> R) -> io::Result<R> {
    match config::read(path).map_err(io::Error::other)? {
        None => {
            let mut machines = Vec::new();
            let result = edit(&mut machines);
            if !machines.is_empty() {
                persist::write(path, &OnlyMachines { machines: &machines }, false)?;
            }
            Ok(result)
        }
        Some(mut config) => {
            let before = config.machines.clone();
            let result = edit(&mut config.machines);
            if config.machines != before {
                config::save(path, &config)?;
            }
            Ok(result)
        }
    }
}

fn pushed(list: &mut Vec<Machine>, destination: String, command: Option<String>, unreached: bool) -> Machine {
    let machine = Machine {
        name: name_for(&destination, list),
        icon: ICONS[0].0.to_string(),
        destination,
        folder: HOME.into(),
        command,
        shown: false,
        unreached,
    };
    list.push(machine.clone());
    machine
}

pub(crate) fn reached(here: &Here, remote: &Remote) -> io::Result<()> {
    if valid_destination(&remote.destination).is_err()
        || valid_command(remote.command.as_deref().unwrap_or_default()).is_err()
    {
        return Ok(());
    }
    change(here, |list| {
        let mut found = false;
        for machine in list.iter_mut().filter(|machine| machine.destination == remote.destination) {
            machine.command.clone_from(&remote.command);
            machine.unreached = false;
            found = true;
        }
        if !found {
            pushed(list, remote.destination.clone(), remote.command.clone(), false);
        }
    })
}

pub(crate) fn remembered(here: &Here, destination: String, command: Option<String>) -> io::Result<Machine> {
    valid_destination(&destination).map_err(io::Error::other)?;
    valid_command(command.as_deref().unwrap_or_default()).map_err(io::Error::other)?;
    change(here, |list| {
        if let Some(machine) = list.iter_mut().find(|machine| machine.destination == destination) {
            machine.command = command;
            return machine.clone();
        }
        pushed(list, destination, command, true)
    })
}

pub(crate) fn forget(here: &Here, destination: &str) -> io::Result<bool> {
    change(here, |list| {
        let before = list.len();
        list.retain(|machine| machine.destination != destination);
        list.len() != before
    })
}

pub(crate) fn name_for(destination: &str, others: &[Machine]) -> String {
    let base = base_name(destination);
    let mut name = base.clone();
    let mut n = 2;
    while name_taken(others, &name).is_some() {
        let suffix = format!("-{n}");
        let kept: String = base.chars().take(NAME_MAX - suffix.len()).collect();
        name = format!("{kept}{suffix}");
        n += 1;
    }
    name
}

fn base_name(destination: &str) -> String {
    let rest = destination.strip_prefix("ssh://").unwrap_or(destination);
    let rest = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    let host = match rest.strip_prefix('[') {
        Some(inside) => inside.split(']').next().unwrap_or_default(),
        None => rest.split(':').next().unwrap_or_default(),
    };
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    let mapped: String = host.chars().map(|c| if allowed(c) { c } else { '-' }).collect();
    let name: String = mapped.trim_matches(['-', '.']).chars().take(NAME_MAX).collect();
    if name.is_empty() || name.eq_ignore_ascii_case(super::EVERY) { FALLBACK_NAME.into() } else { name }
}

fn legacy_machines(file: &Path) -> io::Result<Vec<Legacy>> {
    let bytes = std::fs::read(file)?;
    let entries: Vec<Value> = serde_json::from_slice(&bytes).map_err(io::Error::from)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<Legacy>(entry).ok())
        .filter(|machine| valid(&machine.destination))
        .collect())
}

fn merge(list: &mut Vec<Machine>, old: Legacy) {
    match list.iter_mut().find(|machine| machine.destination == old.destination) {
        Some(machine) => {
            if machine.command.is_none() {
                machine.command = old.command;
            }
        }
        None => {
            pushed(list, old.destination, old.command, old.unreached);
        }
    }
}

pub(crate) fn migrate(here: &Here) -> io::Result<()> {
    if !here.legacy.exists() {
        return Ok(());
    }
    let legacy = legacy_machines(&here.legacy)?;
    edit_config(&here.config, |list| legacy.into_iter().for_each(|old| merge(list, old)))?;
    match std::fs::remove_file(&here.legacy) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
