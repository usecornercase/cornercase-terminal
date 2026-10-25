use std::fmt::Write as _;

use super::args::ports::PortsArgs;
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::ports::PortInfo;
use crate::control::{Command, Listing, Ports};
use crate::error::Result;

pub(super) fn run(args: &PortsArgs) -> Result<()> {
    let workspace = workspace("ports", args.workspace, args.worktree.as_deref())?;
    let value = ask("ports", Command::Ports(Ports { workspace, all: args.all }))?;
    let listing: Listing = answer(value.clone())?;
    if args.print.json { print_json(&value) } else { print_out(&text(&listing)) }
}

pub(crate) fn text(listing: &Listing) -> String {
    if listing.ports.is_empty() {
        return "nothing is listening on a port\n".into();
    }
    listing.ports.iter().map(line).collect()
}

fn line(port: &PortInfo) -> String {
    let mut text = format!("{}  {}", port.url, port.addresses.join(" "));
    let program = port.args.join(" ");
    let program = if program.is_empty() { port.program.clone().unwrap_or_default() } else { program };
    let _ = write!(text, "  {program}  pid {}", port.pid);
    let place = match (port.via.as_deref(), port.agent.as_deref(), port.pane) {
        (Some("agent"), Some(agent), Some(pane)) => format!("agent {agent} in pane {pane}"),
        (Some("cwd"), ..) => "cwd".into(),
        (_, _, Some(pane)) => format!("pane {pane}"),
        _ => String::new(),
    };
    let workspace = port.workspace.map_or_else(|| "no workspace".into(), |id| format!("workspace {id}"));
    let place = if place.is_empty() { workspace } else { format!("{workspace}, {place}") };
    let _ = writeln!(text, "  {place}");
    text
}

#[cfg(test)]
mod tests;
