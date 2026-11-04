use clap::Args;

use super::{Print, branch};
use crate::cli::help::{BY_BRANCH, HERE_WORKSPACE};

#[derive(Debug, Args)]
pub(crate) struct PullRequestArgs {
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}
