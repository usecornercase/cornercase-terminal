use std::fmt::Write as _;

use super::args::pull_request::PullRequestArgs;
use super::status::pull_request_part;
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::{CheckInfo, Command, PullRequestAnswer, PullRequestRead};
use crate::error::Result;
use crate::pull_request::Provider;

const OUTCOME_WIDTH: usize = 7;

pub(super) fn run(args: &PullRequestArgs) -> Result<bool> {
    let workspace = workspace("pull-request", args.workspace, args.worktree.as_deref())?;
    let value = ask("pull-request", Command::PullRequest(PullRequestRead { workspace }))?;
    let read: PullRequestAnswer = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&read))?;
    }
    Ok(true)
}

pub(crate) fn text(read: &PullRequestAnswer) -> String {
    let Some(pull) = &read.pull_request else {
        return format!("{}\n", read.reason.as_deref().unwrap_or("no pull request"));
    };
    let mut text = format!("{}\n{}\n{}\n", pull_request_part(pull), pull.title, pull.url);
    let checks = pull.all_checks.as_deref().unwrap_or_default();
    if checks.is_empty() {
        let _ = writeln!(text, "no {}", Provider::of(&pull.provider).checks());
    }
    for check in checks {
        text.push_str(&check_line(check));
    }
    text
}

fn check_line(check: &CheckInfo) -> String {
    let mut line = format!("{:<OUTCOME_WIDTH$}  {}", check.outcome, check.name);
    for part in [&check.workflow, &check.url].into_iter().filter(|part| !part.is_empty()) {
        let _ = write!(line, "  {part}");
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests;
