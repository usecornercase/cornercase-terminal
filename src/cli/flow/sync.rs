use std::fmt::Write as _;

use super::super::args::FlowSyncArgs;
use super::super::{print_json, print_out};
use crate::client::{self, answer, ask};
use crate::control::{self, FlowsSynced, MachineFlows};
use crate::error::{Error, Result};
use crate::flows::share;

pub(super) fn run(args: &FlowSyncArgs) -> Result<()> {
    client::use_flow_sync()?;
    let request = control::FlowSync { machine: args.machine.clone(), name: args.name.clone() };
    let value = ask("flow sync", control::Command::FlowSync(request))?;
    let synced: FlowsSynced = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&synced_text(&synced))?;
    }
    match synced.machines.iter().find(|machine| machine.error.is_some() || !machine.failed.is_empty()) {
        Some(machine) => Err(Error::Control(format!("{} was not fully synced", share::visible(&machine.machine)))),
        None => Ok(()),
    }
}

fn machine_line(machine: &MachineFlows) -> String {
    let mut line = format!("{}: {}\n", share::visible(&machine.machine), machine.outcome());
    for failed in &machine.failed {
        let _ = writeln!(line, "    {}: {}", share::visible(&failed.name), share::visible(&failed.error));
    }
    line
}

pub(super) fn synced_text(synced: &FlowsSynced) -> String {
    synced.machines.iter().map(machine_line).collect()
}
