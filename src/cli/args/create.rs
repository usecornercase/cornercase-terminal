use std::path::PathBuf;

use clap::builder::NonEmptyStringValueParser;
use clap::{ArgGroup, Args};

use super::{Print, ViewArg, accounts, branch, machine};
use crate::cli::help::{BY_BRANCH, FRESH_HELP, HERE_PANE, HERE_WORKSPACE, NO_FRESH_HELP};

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
pub(crate) struct FreshArg {
    #[arg(long, help = FRESH_HELP)]
    pub(crate) fresh: bool,
    #[arg(long, conflicts_with = "fresh", help = NO_FRESH_HELP)]
    pub(crate) no_fresh: bool,
}

impl FreshArg {
    pub(crate) fn asked(&self) -> Option<bool> {
        match (self.fresh, self.no_fresh) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
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
#[command(
    override_usage = "cornercase clone [OPTIONS] <URL> [FOLDER]\n       \
                      cornercase clone --list-ignored [--from <FOLDER>] [--json]",
    group(ArgGroup::new("read_here").args(["files", "list_ignored"]).multiple(true))
)]
pub(crate) struct CloneArgs {
    #[arg(
        value_name = "URL",
        value_parser = NonEmptyStringValueParser::new(),
        required_unless_present = "list_ignored",
        help = "The repository, as `git clone` takes it"
    )]
    pub(crate) url: Option<String>,
    #[arg(
        value_name = "FOLDER",
        help = "The folder the clone goes in, as a new folder named after the repository [default: the current one; \
                with --remote, the home folder there]"
    )]
    pub(crate) folder: Option<PathBuf>,
    #[arg(
        long = "file",
        value_name = "PATH",
        allow_hyphen_values = true,
        help = "A file of the repository here to copy into the clone, such as the git-ignored .env, by its path from \
                the repository's root; repeat it for more"
    )]
    pub(crate) files: Vec<PathBuf>,
    #[arg(
        long,
        value_name = "FOLDER",
        requires = "read_here",
        help = "The repository here that --file and --list-ignored read, on this machine also with --remote \
                [default: the current folder]"
    )]
    pub(crate) from: Option<PathBuf>,
    #[arg(
        long,
        conflicts_with_all = ["url", "folder", "files", "focus", "view"],
        help = "Print the git-ignored files in that folder that --file can copy, and clone nothing; it needs no \
                server, and --remote changes nothing for it"
    )]
    pub(crate) list_ignored: bool,
    #[command(flatten)]
    pub(crate) create: Create,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("branch_from").args(["fresh", "no_fresh"]).requires("worktree")))]
pub(crate) struct NewWorkspaceArgs {
    #[arg(value_name = "NAME", help = "Its name, or its branch with --worktree")]
    pub(crate) name: String,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[arg(long, help = "Check the branch out in a git worktree of its own, at a repository's root only")]
    pub(crate) worktree: bool,
    #[command(flatten)]
    pub(crate) fresh: FreshArg,
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
