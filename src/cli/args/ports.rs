use clap::{Args, Subcommand};

use super::{Print, branch};
use crate::cli::help::{BY_BRANCH, HERE_WORKSPACE, PORTS_STOP_HELP};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct PortsArgs {
    #[command(subcommand)]
    pub(crate) action: Option<PortsAction>,
    #[arg(long, value_name = "ID", conflicts_with = "all", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with_all = ["workspace", "all"], help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, help = "Every TCP port your processes listen on, each with its workspace or none")]
    pub(crate) all: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PortsAction {
    #[command(
        about = "Stop the program of a workspace that listens on a port: SIGTERM, then SIGKILL after 3 s",
        after_help = PORTS_STOP_HELP
    )]
    Stop(StopArgs),
}

#[derive(Debug, Args)]
pub(crate) struct StopArgs {
    #[arg(value_name = "PORT", help = "The port, as `cornercase ports` lists it")]
    pub(crate) port: u16,
    #[arg(long, value_name = "PID", help = "Which program, when several listen on the port")]
    pub(crate) pid: Option<i32>,
    #[command(flatten)]
    pub(crate) print: Print,
}
