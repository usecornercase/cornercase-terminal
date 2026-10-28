use clap::{Args, Subcommand};

use super::{Print, branch};
use crate::cli::help::{BY_BRANCH, PROCESSES_STOP_HELP};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct ProcessesArgs {
    #[command(subcommand)]
    pub(crate) action: Option<ProcessesAction>,
    #[arg(long, value_name = "ID", conflicts_with_all = ["workspace", "worktree"], help = "Only this pane [default: every pane]")]
    pub(crate) pane: Option<u64>,
    #[arg(long, value_name = "ID", help = "Only the panes of this workspace [default: every pane]")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(
        long,
        value_name = "MS",
        value_parser = clap::value_parser!(u64).range(100..=10_000),
        help = "How long to measure the CPU over, in milliseconds [default: 1000]"
    )]
    pub(crate) interval: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ProcessesAction {
    #[command(
        about = "Stop a program of a pane and what it started: SIGTERM, then SIGKILL after 3 s",
        after_help = PROCESSES_STOP_HELP
    )]
    Stop(StopArgs),
}

#[derive(Debug, Args)]
pub(crate) struct StopArgs {
    #[arg(value_name = "PID", help = "The process, as `cornercase processes` lists it")]
    pub(crate) pid: i32,
    #[arg(long, value_name = "STARTED", help = "Only if it is still the process listed with this `started` value")]
    pub(crate) started: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}
