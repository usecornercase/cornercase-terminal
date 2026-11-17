use std::fmt::Write as _;
use std::io;

use serde::Serialize;

use super::{print_json, print_out};
use crate::error::{Error, Result};
use crate::machines::list::{self, Here};
use crate::machines::{self, Machine};
use crate::text::{cells, escape_hidden, pad_cells};

const NOT_REACHED: &str = "  (not reached yet)";
const NONE: &str =
    "No machine on this computer's list yet: `cornercase remote DESTINATION` or Settings › Machines adds one.";

#[derive(Serialize)]
struct Known<'a> {
    machines: Vec<Reached<'a>>,
}

#[derive(Serialize)]
struct Reached<'a> {
    name: &'a str,
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
                name: &machine.name,
                destination: &machine.destination,
                command: machine.command.as_deref(),
                unreached: machine.unreached,
            })
            .collect();
        Self { machines }
    }
}

pub(super) fn run(json: bool) -> Result<()> {
    let known: Vec<Machine> = machines::accepted(&list::read(&Here::this_computer())).cloned().collect();
    if json {
        return print_json(&Known::from(known.as_slice()));
    }
    print_out(&known_text(&known))
}

fn known_text(known: &[Machine]) -> String {
    if known.is_empty() {
        return format!("{NONE}\n");
    }
    let names: Vec<String> = known.iter().map(|machine| escape_hidden(&machine.name)).collect();
    let destinations: Vec<String> = known.iter().map(|machine| escape_hidden(&machine.destination)).collect();
    let widest = |texts: &[String]| texts.iter().map(|text| cells(text)).max().unwrap_or(0);
    let (name_width, width) = (widest(&names), widest(&destinations));
    let mut text = String::new();
    for ((machine, name), destination) in known.iter().zip(&names).zip(&destinations) {
        let _ = write!(text, "{}  ", pad_cells(name, name_width));
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
    print_out(&remembered(&Here::this_computer(), destination, command)?)
}

pub(super) fn forget(destination: &str) -> Result<()> {
    print_out(&forgotten(&Here::this_computer(), destination)?)
}

fn remembered(here: &Here, destination: String, command: Option<String>) -> Result<String> {
    let shown = escape_hidden(&destination);
    let kept = list::remembered(here, destination, command).map_err(|error| refusal(here, &error))?;
    Ok(if kept.unreached {
        format!("remembered {shown}; a window asks before it first connects there\n")
    } else {
        format!("remembered {shown}\n")
    })
}

fn forgotten(here: &Here, destination: &str) -> Result<String> {
    let shown = escape_hidden(destination);
    if list::forget(here, destination).map_err(|error| unwritten(here, &error))? {
        return Ok(format!("forgot {shown}\n"));
    }
    Err(Error::Control(format!(
        "`{shown}` is not on this computer's list; `cornercase remote --known` prints the list"
    )))
}

fn refusal(here: &Here, error: &io::Error) -> Error {
    if error.kind() == io::ErrorKind::InvalidInput {
        return Error::Control(error.to_string());
    }
    unwritten(here, error)
}

fn unwritten(here: &Here, error: &io::Error) -> Error {
    let file = here.config.display();
    Error::Control(format!("cannot read or write `{file}`: {error}; it is left as it is, fix it"))
}

#[cfg(test)]
mod tests;
