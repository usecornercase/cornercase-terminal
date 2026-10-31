use std::fmt::Write as _;
use std::time::SystemTime;

use super::args::StatsArgs;
use super::machines::read_line;
use super::{print_json, print_out};
use crate::agents::stats::{self, View};
use crate::client::{answer, ask, use_machine_usage};
use crate::control::usage::{MachineRead, MachineStats};
use crate::control::{Command, Stats};
use crate::error::{Error, Result};
use crate::machines::HERE_ICON;
use crate::paths;
use crate::text;

const NOT_SUMMED: &str = "not summed";
const PEAK: &str = "most at once: the largest peak of one machine, not a sum";

pub(super) fn run(args: &StatsArgs) -> Result<bool> {
    let now = SystemTime::now();
    if let Some(machine) = &args.machine {
        return machine_stats(args, machine, now);
    }
    let view = match ask("stats", Command::Stats(Stats::default())) {
        Ok(value) => answer::<View>(value)?.settled(now),
        Err(Error::NoServer) => stats::load(&paths::stats(&paths::session()), now).view(now),
        Err(e) => return Err(e),
    };
    if args.json {
        print_json(&view)?;
    } else {
        print_out(&(shown(&view, args.share) + "\n"))?;
    }
    Ok(true)
}

fn machine_stats(args: &StatsArgs, machine: &str, now: SystemTime) -> Result<bool> {
    use_machine_usage()?;
    let value = ask("stats", Command::Stats(Stats { machine: Some(machine.into()) }))?;
    if args.json {
        print_json(&value)?;
        return Ok(true);
    }
    let mut read: MachineStats = answer(value)?;
    read.view = read.view.settled(now);
    print_out(&(machine_text(&read, args.share, now) + "\n"))?;
    Ok(true)
}

fn shown(view: &View, share: bool) -> String {
    if share { view.share_link() } else { view.text() }
}

pub(crate) fn machine_text(read: &MachineStats, share: bool, now: SystemTime) -> String {
    let view = shown(&read.view, share);
    if share {
        return view;
    }
    match &read.machine {
        Some(machine) => format!("{}\n\n{view}", read_line(machine, 0, now)),
        None if read.machines.is_empty() => view,
        None => format!("{view}\n\n{}", summed(&read.machines, now)),
    }
}

fn summed(machines: &[MachineRead], now: SystemTime) -> String {
    let width = machines.iter().map(|machine| text::cells(&machine.name)).max().unwrap_or_default();
    let mut text = format!("{PEAK}\nsummed from:\n{HERE_ICON} this computer");
    for machine in machines {
        let left_out = if machine.read_at.is_none() { format!(" · {NOT_SUMMED}") } else { String::new() };
        let _ = write!(text, "\n{}{left_out}", read_line(machine, width, now));
    }
    text
}

#[cfg(test)]
mod tests;
