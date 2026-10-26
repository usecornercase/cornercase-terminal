use std::fmt::Write as _;

use super::args::usage::UsageArgs;
use super::{partial_unless, print_json, print_out};
use crate::agents::usage::{until, updated};
use crate::client::{answer, ask};
use crate::control::usage::PlanInfo;
use crate::control::{Command, Plans, Usage};
use crate::error::Result;
use crate::time::{self, Stamp};

pub(super) fn run(args: &UsageArgs) -> Result<bool> {
    let value = ask("usage", Command::Usage(Usage { cached: args.cached }))?;
    let plans: Plans = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&plans, time::now()))?;
    }
    partial_unless(plans.agents.iter().all(|plan| plan.error.is_none()))
}

pub(crate) fn text(plans: &Plans, clock: i64) -> String {
    plans.agents.iter().map(|plan| agent(plan, clock)).collect::<Vec<_>>().join("\n")
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
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
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
