use std::path::PathBuf;

use clap::{ArgGroup, Args};

use super::{Create, FreshArg, UntilArg, accounts, issue, machine, seconds};
use crate::cli::help::{HERE_WORKSPACE, UNTIL_AFTER};

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("branch_from").args(["fresh", "no_fresh"]).conflicts_with("workspace")))]
pub(crate) struct StartArgs {
    #[arg(
        value_name = "AGENT",
        help = "The agent, such as claude or codex [default: the one chosen in settings → agents, else the one \
                running here]"
    )]
    pub(crate) agent: Option<String>,
    #[arg(long, value_name = "MODEL", help = "The model, such as opus or gpt-5.5 [default: the agent's own]")]
    pub(crate) model: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "One of the agent's modes, as settings → agents lists them, such as plan, or default for no mode \
                arguments [default: the one chosen there]"
    )]
    pub(crate) mode: Option<String>,
    #[arg(
        long,
        value_name = "LEVEL",
        help = "The agent's effort, such as high: Claude Code, Codex, Grok Build and GitHub Copilot CLI take one [default: the agent's own]"
    )]
    pub(crate) effort: Option<String>,
    #[arg(
        long,
        value_name = "BRANCH",
        conflicts_with = "workspace",
        help = "Start it in the worktree of this branch, made when missing"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(
        long,
        value_name = "ID",
        conflicts_with = "workspace",
        help = "The project: a new tab in its folder, or where --worktree or --issue makes the worktree [default: the one this runs in, else the shown one]"
    )]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "The tab's name [default: the agent's]")]
    pub(crate) name: Option<String>,
    #[arg(long, value_name = "TEXT", conflicts_with_all = ["prompt_file", "issue"], help = "What to ask it")]
    pub(crate) prompt: Option<String>,
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "issue",
        help = "Read the prompt from a file, or from standard input with -"
    )]
    pub(crate) prompt_file: Option<PathBuf>,
    #[arg(long, help = "Then wait until the agent stops working, and print how it ended")]
    pub(crate) wait: bool,
    #[arg(long, value_enum, requires = "wait", help = UNTIL_AFTER)]
    pub(crate) until: Option<UntilArg>,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, help = "Give up after this long, with status 1")]
    pub(crate) timeout: Option<f64>,
    #[arg(
        long,
        conflicts_with_all = ["wait", "until", "timeout"],
        help = "Answer as soon as the tab is open, with the same ids, while the launch goes on: for a program that shows the new tab itself"
    )]
    pub(crate) placed: bool,
    #[command(flatten)]
    pub(crate) fresh: FreshArg,
    #[command(flatten)]
    pub(crate) issue: issue::IssueArg,
    #[command(flatten)]
    pub(crate) profile: accounts::ProfileArg,
    #[command(flatten)]
    pub(crate) on: machine::MachineArg,
    #[command(flatten)]
    pub(crate) create: Create,
}
