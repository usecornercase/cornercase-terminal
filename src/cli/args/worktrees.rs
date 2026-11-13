use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::{BootArg, Print, branch};
use crate::cli::help::{
    BY_BRANCH, HERE_WORKSPACE, WORKTREES_FREE_HELP, WORKTREES_LIST_HELP, WORKTREES_REMOVE_HELP, WORKTREES_SIZE_HELP,
};

#[derive(Debug, Args)]
pub(crate) struct WorktreesArgs {
    #[command(subcommand)]
    pub(crate) action: WorktreesAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorktreesAction {
    #[command(
        about = "List every worktree on disk with what makes it safe to remove and its size",
        after_help = WORKTREES_LIST_HELP
    )]
    List(ListArgs),
    #[command(about = "Measure the disk a worktree takes and its build folders", after_help = WORKTREES_SIZE_HELP)]
    Size(TargetArgs),
    #[command(
        about = "Delete the git-ignored build folders of a worktree and keep the worktree",
        after_help = WORKTREES_FREE_HELP
    )]
    FreeBuilds(FreeArgs),
    #[command(
        about = "Remove worktrees as the Worktrees dialog does, open or not, and their branches when asked",
        after_help = WORKTREES_REMOVE_HELP
    )]
    Remove(RemoveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct TargetArgs {
    #[arg(long, value_name = "ID", conflicts_with_all = ["worktree", "path"], help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with_all = ["workspace", "path"], help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "PATH", conflicts_with_all = ["workspace", "worktree"], help = "A linked worktree by its folder, open in cornercase or not")]
    pub(crate) path: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FreeArgs {
    #[command(flatten)]
    pub(crate) target: TargetArgs,
    #[arg(long, help = "Print what would be freed and what runs there, and delete nothing")]
    pub(crate) check: bool,
    #[arg(long, help = "Go on while a program runs in the worktree or a build folder holds a git checkout")]
    pub(crate) force: bool,
    #[command(flatten)]
    pub(crate) boot: BootArg,
}

#[derive(Debug, Args)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own option of worktrees remove")]
pub(crate) struct RemoveArgs {
    #[arg(
        long = "path",
        value_name = "FOLDER",
        required = true,
        num_args = 1..,
        help = "The worktrees by their folders, as `cornercase worktrees list` names them"
    )]
    pub(crate) paths: Vec<PathBuf>,
    #[arg(
        long,
        conflicts_with_all = ["force", "unlock"],
        help = "Print what the Worktrees dialog's confirmation names for each folder, and stop and delete nothing"
    )]
    pub(crate) check: bool,
    #[arg(long, help = "Go on when a folder has uncommitted files, programs in it or commits that would be lost")]
    pub(crate) force: bool,
    #[arg(long, help = "Unlock a locked worktree and remove it")]
    pub(crate) unlock: bool,
    #[arg(long, help = "Also delete the branch checked out there, never the default branch or the main checkout's")]
    pub(crate) delete_branch: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct ListArgs {
    #[arg(
        long,
        value_name = "ID",
        help = "Only the worktrees of this project's repository [default: every worktree on disk]"
    )]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Leave out the sizes, so it answers without walking the worktrees")]
    pub(crate) no_sizes: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

impl WorktreesAction {
    pub(crate) fn boot(&self) -> Option<&BootArg> {
        match self {
            Self::FreeBuilds(a) => Some(&a.boot),
            _ => None,
        }
    }
}
