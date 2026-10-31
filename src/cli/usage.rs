use std::fmt::Write as _;
use std::time::{Duration, UNIX_EPOCH};

use super::args::usage::UsageArgs;
use super::machines::read_line;
use super::{partial_unless, print_json, print_out};
use crate::agents::usage::{until, updated};
use crate::client::{answer, ask, use_machine_usage};
use crate::control::usage::PlanInfo;
use crate::control::{Command, Plans, Usage};
use crate::error::Result;
use crate::machines::HERE_ICON;
use crate::time::{self, Stamp};

pub(super) fn run(args: &UsageArgs) -> Result<bool> {
    if args.machine.is_some() {
        use_machine_usage()?;
    }
    let value = ask("usage", Command::Usage(Usage { cached: args.cached, machine: args.machine.clone() }))?;
    let plans: Plans = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&every_text(&plans, time::now()))?;
    }
    partial_unless(answered(&plans))
}

fn answered(plans: &Plans) -> bool {
    let mut machines = plans.machine.iter().chain(plans.machines.iter().map(|there| &there.machine));
    let mut agents = plans.agents.iter().chain(plans.machines.iter().flat_map(|there| &there.agents));
    machines.all(|machine| machine.error.is_none() && machine.read_at.is_some())
        && agents.all(|plan| plan.error.is_none())
}

fn every_text(plans: &Plans, clock: i64) -> String {
    let now = UNIX_EPOCH + Duration::from_secs(u64::try_from(clock).unwrap_or_default());
    let here = text(&plans.agents, clock);
    if let Some(machine) = &plans.machine {
        return format!("{}\n{here}", read_line(machine, 0, now));
    }
    if plans.machines.is_empty() {
        return here;
    }
    let mut every = format!("{HERE_ICON} this computer\n{here}");
    for there in &plans.machines {
        let _ = write!(every, "\n{}\n{}", read_line(&there.machine, 0, now), text(&there.agents, clock));
    }
    every
}

pub(crate) fn text(agents: &[PlanInfo], clock: i64) -> String {
    agents.iter().map(|plan| agent(plan, clock)).collect::<Vec<_>>().join("\n")
}

fn agent(plan: &PlanInfo, clock: i64) -> String {
    let mut text = plan.heading();
    if let Some(name) = &plan.plan {
        let _ = write!(text, " · {name} plan");
    }
    let status = if plan.loading { "loading…".into() } else { plan.age.map(updated).unwrap_or_default() };
    if !status.is_empty() {
        let _ = write!(text, "  {status}");
    }
    text.push('\n');
    if let Some(error) = &plan.error {
        let _ = writeln!(text, "  usage unavailable: {error}");
    }
    let width = plan.limits.iter().map(|limit| limit.label.chars().count()).max().unwrap_or(0);
    for limit in &plan.limits {
        let resets = limit
            .resets_at
            .as_deref()
            .and_then(Stamp::parse)
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map(|at| format!(" · {}", until(i64::try_from(at.as_secs()).unwrap_or(i64::MAX) - clock)))
            .unwrap_or_default();
        let _ = writeln!(text, "  {:<width$}  {:>3}%{resets}", limit.label, limit.percent);
    }
    if let Some(notice) = &plan.notice {
        let _ = writeln!(text, "  {notice}");
    }
    if let Some(extra) = &plan.extra {
        let _ = writeln!(text, "  {extra}");
    }
    if plan.read_at.is_none() && plan.error.is_none() && !plan.loading {
        text.push_str("  not asked yet\n");
    }
    text
}

#[cfg(test)]
mod tests;
