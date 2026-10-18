use clap::{Args, Subcommand, ValueEnum};

use super::{Print, branch};
use crate::changes::Mode;
use crate::cli::help::{BY_BRANCH, HERE_WORKSPACE};

#[derive(Debug, Args)]
pub(crate) struct ChangesArgs {
    #[command(subcommand)]
    pub(crate) action: ChangesAction,
}

#[derive(Debug, Args)]
pub(crate) struct Where {
    #[arg(long, value_name = "ID", help = HERE_WORKSPACE)]
    pub(crate) workspace: Option<u64>,
    #[arg(long, value_name = "BRANCH", value_parser = branch, conflicts_with = "workspace", help = BY_BRANCH)]
    pub(crate) worktree: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum ModeArg {
    #[default]
    Uncommitted,
    Commits,
    All,
}

impl From<ModeArg> for Mode {
    fn from(mode: ModeArg) -> Self {
        match mode {
            ModeArg::Uncommitted => Self::Uncommitted,
            ModeArg::Commits => Self::Commits,
            ModeArg::All => Self::All,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ModeFlag {
    #[arg(long, value_enum, default_value_t, help = "What to compare: uncommitted, commits or all")]
    pub(crate) mode: ModeArg,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ChangesAction {
    #[command(about = "Print the changed files of a workspace, or of every project")]
    List {
        #[command(flatten)]
        place: Where,
        #[arg(
            long,
            conflicts_with_all = ["workspace", "worktree", "mode"],
            help = "Every project's uncommitted changes, grouped like the panel's Every project"
        )]
        every: bool,
        #[command(flatten)]
        mode: ModeFlag,
        #[arg(long, help = "Keep running and print the files as one JSON line each time they change")]
        follow: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Print one file's diff")]
    File {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        mode: ModeFlag,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Print the unchanged lines before a hunk of a file")]
    Gap {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[arg(
            long,
            value_name = "N",
            required = true,
            help = "The hunk, counted from 1: its gap is the lines before it"
        )]
        hunk: usize,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        mode: ModeFlag,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "List the branches to compare with, or choose one")]
    Base {
        #[arg(value_name = "BRANCH", help = "Compare with this branch")]
        branch: Option<String>,
        #[arg(long = "default", conflicts_with = "branch", help = "Go back to following the default branch")]
        default: bool,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Mark a file as viewed, or clear the mark")]
    Viewed {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[arg(long, help = "Clear the mark")]
        clear: bool,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        mode: ModeFlag,
    },
}
