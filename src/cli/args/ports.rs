use clap::Args;

use super::{Print, branch};
use crate::cli::help::{BY_BRANCH, HERE_WORKSPACE};

#[derive(Debug, Args)]
pub(crate) struct PortsArgs {
    #[arg(long, value_name = "ID", conflicts_with = "all", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with_all = ["workspace", "all"], help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, help = "Every TCP port your processes listen on, each with its workspace or none")]
    pub(crate) all: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}
