use clap::{Args, Subcommand, ValueEnum};

use super::{Print, branch};
use crate::changes::Mode;
use crate::cli::help::{BY_BRANCH, HERE_WORKSPACE, SEEN_DIGEST};
use crate::control::changes::parse_digest;

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

fn digest(text: &str) -> Result<String, String> {
    parse_digest(text)
        .map(|_| text.to_string())
        .ok_or_else(|| "a digest is 16 hexadecimal digits, as `changes list --json` prints it".into())
}

#[derive(Debug, Args)]
pub(crate) struct DigestFlag {
    #[arg(long, value_name = "HEX", value_parser = digest, help = SEEN_DIGEST)]
    pub(crate) digest: Option<String>,
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
    #[command(about = "Print the unchanged lines between a hunk of a file and the one before")]
    Gap {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[arg(
            long,
            value_name = "N",
            required = true,
            help = "The hunk, counted from 0 as in `changes file`: its gap is the lines between it and the one before"
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
        #[arg(long, value_name = "HEX", value_parser = digest, conflicts_with_all = ["clear", "mode"], help = SEEN_DIGEST)]
        digest: Option<String>,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        mode: ModeFlag,
    },
    #[command(about = "Throw away a file's uncommitted changes, after asking")]
    Discard {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        digest: DigestFlag,
        #[arg(short, long, help = "Discard without asking")]
        yes: bool,
    },
    #[command(about = "Put a file's changes in git's index, as `git add` does")]
    Stage {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        digest: DigestFlag,
    },
    #[command(about = "Take a file's changes out of git's index; the file keeps them")]
    Unstage {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace")]
        path: String,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        digest: DigestFlag,
    },
}
