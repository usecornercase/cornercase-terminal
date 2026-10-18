use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use super::issue;
use super::{Create, Print, branch};
use crate::cli::help::{FLOW_EXPORT_HELP, FLOW_IMPORT_HELP, FLOW_RUN_HELP};

#[derive(Debug, Args)]
pub(crate) struct FlowArgs {
    #[command(subcommand)]
    pub(crate) action: FlowAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum FlowAction {
    #[command(about = "List the flows, built in, yours and the project's, with their agents and their problems")]
    List(FlowListArgs),
    #[command(
        about = "Start a flow in a worktree, and print its workspace's id and each role's pane id",
        after_help = FLOW_RUN_HELP
    )]
    Run(FlowRunArgs),
    #[command(
        about = "Print a flow as text to share it: a file to send to a teammate, or to commit",
        after_help = FLOW_EXPORT_HELP
    )]
    Export(FlowExportArgs),
    #[command(
        about = "Add a flow someone shared, from a file or from standard input with -",
        after_help = FLOW_IMPORT_HELP
    )]
    Import(FlowImportArgs),
}

#[derive(Debug, Args)]
pub(crate) struct FlowExportArgs {
    #[arg(value_name = "NAME", help = "The flow, as `cornercase flow list` names it")]
    pub(crate) name: String,
    #[arg(long, value_name = "ID", help = "The project whose flows to look in [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ImportTo {
    #[value(help = "Your flows, in flows/ next to config.json")]
    Yours,
    #[value(help = "The project's, in .cornercase/flows/ in its repository")]
    Project,
}

impl ImportTo {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Project => "project",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct FlowImportArgs {
    #[arg(
        value_name = "SOURCE",
        help = "A file `flow export` made, - for standard input, an https:// address, or OWNER/REPO on GitHub (usecornercase.dev/flows)"
    )]
    pub(crate) source: String,
    #[arg(long, value_enum, default_value = "yours", help = "Where the flow goes")]
    pub(crate) to: ImportTo,
    #[arg(long, value_name = "ID", help = "The project, for --to project [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[arg(long, value_name = "NAME", help = "Import it under another name")]
    pub(crate) name: Option<String>,
    #[arg(long, help = "Overwrite the flow that has its name")]
    pub(crate) replace: bool,
    #[arg(
        short = 'y',
        long,
        help = "Import without asking, though the flow comes from the web or sets variables or arguments"
    )]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowListArgs {
    #[arg(
        long,
        value_name = "ID",
        help = "The project whose flows to list too [default: the one this runs in, else the shown one]"
    )]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowRunArgs {
    #[arg(value_name = "NAME", help = "The flow, as `cornercase flow list` names it")]
    pub(crate) name: String,
    #[arg(
        long,
        value_name = "TEXT",
        conflicts_with = "prompt_file",
        required_unless_present_any = ["prompt_file", "issue"],
        help = "The task: what the team should do"
    )]
    pub(crate) prompt: Option<String>,
    #[arg(long, value_name = "FILE", help = "Read the task from a file, or from standard input with -")]
    pub(crate) prompt_file: Option<PathBuf>,
    #[arg(
        long,
        value_name = "BRANCH",
        value_parser = branch,
        help = "The worktree's branch, made when missing [default: from the task's first words, made unique]"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) issue: issue::IssueArg,
    #[command(flatten)]
    pub(crate) create: Create,
}
