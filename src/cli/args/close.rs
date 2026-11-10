use clap::{ArgGroup, Args};

use super::{BootArg, ViewArg, branch};
use crate::cli::help::BY_BRANCH;

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(crate) struct Which {
    #[arg(long, value_name = "ID", help = "A pane")]
    pub(crate) pane: Option<u64>,
    #[arg(long, value_name = "ID", help = "A tab and its panes")]
    pub(crate) tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "A workspace and its tabs")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "A project and its workspaces")]
    pub(crate) project: Option<u64>,
}

#[derive(Debug, Args)]
#[command(
    group(ArgGroup::new("removing").multiple(false).args(["remove_worktree", "agent"])),
    group(ArgGroup::new("printing").multiple(true).args(["check", "agent"]))
)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own option of close")]
pub(crate) struct CloseArgs {
    #[command(flatten)]
    pub(crate) which: Which,
    #[arg(
        long,
        conflicts_with_all = ["pane", "tab", "project"],
        help = "Also remove the workspace's git worktree from disk"
    )]
    pub(crate) remove_worktree: bool,
    #[arg(
        long,
        conflicts_with_all = ["tab", "workspace", "worktree", "project"],
        help = "Close the agent in --pane with what it started there, as its close agent in the window does; nothing is removed from disk"
    )]
    pub(crate) agent: bool,
    #[arg(
        long,
        requires = "remove_worktree",
        conflicts_with = "agent",
        help = "Remove it even when git would lose changes"
    )]
    pub(crate) force: bool,
    #[arg(
        long,
        requires = "remove_worktree",
        conflicts_with = "agent",
        help = "Unlock a locked worktree first, as the window's unlock and remove does; changes still need --force"
    )]
    pub(crate) unlock: bool,
    #[arg(
        long,
        requires = "removing",
        conflicts_with_all = ["force", "unlock"],
        help = "Only say what removing it would lose, or with --agent what would close, as the window's dialog does; nothing stops"
    )]
    pub(crate) check: bool,
    #[arg(long, requires = "printing", help = "Print the check, or with --agent what was closed, as JSON")]
    pub(crate) json: bool,
    #[command(flatten)]
    pub(crate) boot: BootArg,
}

#[derive(Debug, Args)]
#[group(multiple = false)]
pub(crate) struct Renamed {
    #[arg(long, value_name = "ID", help = "The tab [default: the one this runs in, else the shown one]")]
    pub(crate) tab: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this workspace instead")]
    pub(crate) workspace: Option<u64>,
    #[arg(
        long,
        value_name = "BRANCH",
        value_parser = branch,
        help = "Rename the workspace on this branch instead, looked up in the project this runs in, else the shown one"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "Rename this project instead")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "ID", help = "Rename this group instead")]
    pub(crate) group: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct RenameArgs {
    #[command(flatten)]
    pub(crate) renamed: Renamed,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) boot: BootArg,
    #[arg(value_name = "NAME", help = "The new name; an empty one goes back to the default")]
    pub(crate) name: String,
}

#[cfg(test)]
mod tests;
