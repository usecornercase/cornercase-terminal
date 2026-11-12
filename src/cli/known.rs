use std::fmt::Write as _;
use std::io;
use std::path::Path;

use serde::Serialize;

use super::{print_json, print_out};
use crate::error::{Error, Result};
use crate::remote::machines::{self, Machine};
use crate::text::{cells, escape_hidden, pad_cells};

const NOT_REACHED: &str = "  (not reached yet)";
const NONE: &str = "No machine reached yet: `cornercase remote DESTINATION` adds one.";

#[derive(Serialize)]
struct Known<'a> {
    machines: Vec<Reached<'a>>,
}

#[derive(Serialize)]
struct Reached<'a> {
    destination: &'a str,
    command: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    unreached: bool,
}

impl<'a> From<&'a [Machine]> for Known<'a> {
    fn from(known: &'a [Machine]) -> Self {
        let machines = known
            .iter()
            .map(|machine| Reached {
                destination: &machine.destination,
                command: machine.command.as_deref(),
                unreached: machine.unreached,
            })
            .collect();
        Self { machines }
    }
}

pub(super) fn run(json: bool) -> Result<()> {
    let known = machines::read(&machines::file());
    if json {
        return print_json(&Known::from(known.as_slice()));
    }
    print_out(&known_text(&known))
}

fn known_text(known: &[Machine]) -> String {
    if known.is_empty() {
        return format!("{NONE}\n");
    }
    let destinations: Vec<String> = known.iter().map(|machine| escape_hidden(&machine.destination)).collect();
    let width = destinations.iter().map(|destination| cells(destination)).max().unwrap_or(0);
    let mut text = String::new();
    for (machine, destination) in known.iter().zip(&destinations) {
        match &machine.command {
            Some(command) => {
                let _ = write!(text, "{}  --command {}", pad_cells(destination, width), escape_hidden(command));
            }
            None => text.push_str(destination),
        }
        text.push_str(if machine.unreached { NOT_REACHED } else { "" });
        text.push('\n');
    }
    text
}

pub(super) fn remember(destination: String, command: Option<String>) -> Result<()> {
    print_out(&remembered(&machines::file(), Machine::never_reached(destination, command))?)
}

pub(super) fn forget(destination: &str) -> Result<()> {
    print_out(&forgotten(&machines::file(), destination)?)
}

fn remembered(file: &Path, machine: Machine) -> Result<String> {
    let destination = escape_hidden(&machine.destination);
    unreadable(file)?;
    let kept = machines::remember(file, machine).map_err(|error| unwritten(file, &error))?;
    Ok(if kept.unreached {
        format!("remembered {destination}; a window asks before it first connects there\n")
    } else {
        format!("remembered {destination}\n")
    })
}

fn forgotten(file: &Path, destination: &str) -> Result<String> {
    let shown = escape_hidden(destination);
    unreadable(file)?;
    if machines::forget(file, destination).map_err(|error| unwritten(file, &error))? {
        return Ok(format!("forgot {shown}\n"));
    }
    Err(Error::Control(format!("`{shown}` is not on this machine's list; `cornercase remote --known` prints the list")))
}

fn unreadable(file: &Path) -> Result<()> {
    machines::readable(file).map_err(|error| {
        Error::Control(format!("cannot read `{}`: {error}; it is left as it is, fix or remove it", file.display()))
    })
}

fn unwritten(file: &Path, error: &io::Error) -> Error {
    Error::Control(format!("cannot write `{}`: {error}", file.display()))
}

#[cfg(test)]
mod tests;
