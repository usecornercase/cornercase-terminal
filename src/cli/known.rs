use std::fmt::Write as _;

use serde::Serialize;

use super::{print_json, print_out};
use crate::error::Result;
use crate::remote::machines::{self, Machine};
use crate::text::{cells, escape_hidden, pad_cells};

const NONE: &str = "No machine reached yet: `cornercase remote DESTINATION` adds one.";

#[derive(Serialize)]
struct Known<'a> {
    machines: Vec<Reached<'a>>,
}

#[derive(Serialize)]
struct Reached<'a> {
    destination: &'a str,
    command: Option<&'a str>,
}

impl<'a> From<&'a [Machine]> for Known<'a> {
    fn from(known: &'a [Machine]) -> Self {
        let machines = known
            .iter()
            .map(|machine| Reached { destination: &machine.destination, command: machine.command.as_deref() })
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
                let _ = writeln!(text, "{}  --command {}", pad_cells(destination, width), escape_hidden(command));
            }
            None => {
                let _ = writeln!(text, "{destination}");
            }
        }
    }
    text
}

#[cfg(test)]
mod tests;
