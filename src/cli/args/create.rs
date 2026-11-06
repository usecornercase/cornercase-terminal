use std::path::PathBuf;

use clap::Args;
use clap::builder::NonEmptyStringValueParser;

use super::{Print, ViewArg, accounts, branch, machine};
use crate::cli::help::{BY_BRANCH, HERE_PANE, HERE_WORKSPACE};

#[derive(Debug, Args)]
pub(crate) struct Create {
    #[arg(long, help = "Also show it in the window, or in the view with --view, which otherwise stay as they are")]
    pub(crate) focus: bool,
    #[command(flatten)]
    pub(crate) view: ViewArg,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct OpenArgs {
    #[arg(value_name = "PATH", help = "The folder, or a VS Code .code-workspace file to import as a group")]
    pub(crate) path: PathBuf,
    #[arg(long, value_name = "GROUP", help = "Put it in this group, as the group's add project does")]
    pub(crate) into: Option<u64>,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct CloneArgs {
    #[arg(value_name = "URL", value_parser = NonEmptyStringValueParser::new(), help = "The repository, as `git clone` takes it")]
    pub(crate) url: String,
    #[arg(
        value_name = "FOLDER",
        help = "The folder the clone goes in, as a new folder named after the repository [default: the current one; \
                with --remote, the home folder there]"
    )]
    pub(crate) folder: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct NewWorkspaceArgs {
    #[arg(value_name = "NAME", help = "Its name, or its branch with --worktree")]
    pub(crate) name: String,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Check the branch out in a git worktree of its own, at a repository's root only")]
    pub(crate) worktree: bool,
    #[command(flatten)]
    pub(crate) on: machine::MachineArg,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
pub(crate) struct NewTabArgs {
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
    #[arg(long, help = "The tab's name [default: the name of the program it runs]")]
    pub(crate) name: Option<String>,
    #[command(flatten)]
    pub(crate) profile: accounts::ProfileArg,
    #[command(flatten)]
    pub(crate) create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into its shell")]
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct SplitArgs {
    #[arg(long, value_name = "ID", help = HERE_PANE)]
    pub(crate) pane: Option<u64>,
    #[arg(long, help = "Split it down instead of right")]
    pub(crate) down: bool,
    #[command(flatten)]
    pub(crate) create: Create,
    #[arg(last = true, value_name = "COMMAND", help = "What to type into the new shell")]
    pub(crate) command: Vec<String>,
}
