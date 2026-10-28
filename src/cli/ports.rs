use std::fmt::Write as _;

use super::args::ports::{PortsAction, PortsArgs, StopArgs};
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::ports::PortInfo;
use crate::control::{Command, Listing, PortStopped, Ports, StopPort};
use crate::error::Result;
use crate::process::command_line;

const COMMAND_WIDTH: usize = 40;

pub(super) fn run(args: &PortsArgs) -> Result<bool> {
    if let Some(PortsAction::Stop(stop)) = &args.action {
        return run_stop(stop);
    }
    let workspace = workspace("ports", args.workspace, args.worktree.as_deref())?;
    let value = ask("ports", Command::Ports(Ports { workspace, all: args.all }))?;
    let listing: Listing = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&listing))?;
    }
    Ok(true)
}

fn run_stop(args: &StopArgs) -> Result<bool> {
    let value = ask("stop-port", Command::StopPort(StopPort { port: args.port, pid: args.pid }))?;
    let stopped: PortStopped = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&stopped_text(&stopped))?;
    }
    Ok(stopped.outcome != "still-running")
}

pub(crate) fn stopped_text(stopped: &PortStopped) -> String {
    let program = stopped.program.as_deref().unwrap_or("the program");
    let what = format!("{program} (pid {}) on port {}", stopped.pid, stopped.port);
    match stopped.outcome.as_str() {
        "stopped" => format!("stopped {what}\n"),
        "killed" => format!("killed {what}: it did not stop within 3 s of SIGTERM\n"),
        _ => format!("{what} is still running after SIGKILL\n"),
    }
}

pub(crate) fn text(listing: &Listing) -> String {
    if listing.ports.is_empty() {
        return "nothing is listening on a port\n".into();
    }
    listing.ports.iter().map(line).collect()
}

fn line(port: &PortInfo) -> String {
    let mut text = format!("{}  {}", port.url, port.addresses.join(" "));
    let program = command_line(port.program.as_deref(), &port.args, COMMAND_WIDTH);
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
