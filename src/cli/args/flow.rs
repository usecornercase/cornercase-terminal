use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use super::changes::Where;
use super::{Create, Print, branch};
use super::{accounts, issue};
use crate::cli::help::{
    FLOW_DEFAULT_HELP, FLOW_DELETE_HELP, FLOW_EXPORT_HELP, FLOW_FILES_HELP, FLOW_HIDE_HELP, FLOW_IMPORT_HELP,
    FLOW_MESSAGES_HELP, FLOW_ORDER_HELP, FLOW_RESTORE_HELP, FLOW_RUN_HELP, FLOW_RUNS_HELP, FLOW_SAVE_HELP,
};

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
    #[command(
        about = "Save a flow given as JSON, as `flow list --json` gives it, checked as Settings › Flows does",
        after_help = FLOW_SAVE_HELP
    )]
    Save(FlowSaveArgs),
    #[command(
        about = "Print where each running flow stands: its stage, its loops, each role's pane and status",
        after_help = FLOW_RUNS_HELP
    )]
    Runs(FlowRunsArgs),
    #[command(
        about = "Print what the orchestrator and the roles of a run sent each other, and follow new messages",
        after_help = FLOW_MESSAGES_HELP
    )]
    Messages(FlowMessagesArgs),
    #[command(about = "List the files of a run's folder, newest first, also after the run ended", after_help = FLOW_FILES_HELP)]
    Files(FlowFilesArgs),
    #[command(
        about = "Delete a flow of yours or the project's, by name or by its file",
        after_help = FLOW_DELETE_HELP
    )]
    Delete(FlowDeleteArgs),
    #[command(
        about = "Delete a built-in flow, as Settings › Flows does; `flow restore` brings it back",
        after_help = FLOW_HIDE_HELP
    )]
    Hide(FlowNameArgs),
    #[command(
        about = "Bring a deleted built-in flow back, and delete your version of it",
        after_help = FLOW_RESTORE_HELP
    )]
    Restore(FlowNameArgs),
    #[command(
        about = "Choose the flow the start dialog opens on, as set default does in Settings › Flows",
        after_help = FLOW_DEFAULT_HELP
    )]
    Default(FlowDefaultArgs),
    #[command(
        about = "Put the flows in your order, as dragging them in Settings › Flows does",
        after_help = FLOW_ORDER_HELP
    )]
    Order(FlowOrderArgs),
}

#[derive(Debug, Args)]
pub(crate) struct FlowDefaultArgs {
    #[arg(
        value_name = "NAME",
        required_unless_present = "clear",
        conflicts_with = "clear",
        help = "The flow, as `cornercase flow list` names it"
    )]
    pub(crate) name: Option<String>,
    #[arg(long, help = "Choose no default: the start dialog opens on the first built-in flow that can start")]
    pub(crate) clear: bool,
    #[arg(long, value_name = "ID", help = "The project whose flows to look in [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowOrderArgs {
    #[arg(
        value_name = "NAME",
        num_args = 1..,
        required_unless_present = "clear",
        conflicts_with = "clear",
        help = "The flows, first to last, as `cornercase flow list` names them"
    )]
    pub(crate) names: Vec<String>,
    #[arg(long, help = "Go back to name order")]
    pub(crate) clear: bool,
    #[arg(long, value_name = "ID", help = "The project whose flows to look in [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowRunsArgs {
    #[arg(long, help = "Print the runs again, as one JSON line, each time something changes")]
    pub(crate) follow: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowMessagesArgs {
    #[command(flatten)]
    pub(crate) place: Where,
    #[arg(long, help = "Go on printing each new message until the run ends")]
    pub(crate) follow: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowFilesArgs {
    #[command(flatten)]
    pub(crate) place: Where,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowNameArgs {
    #[arg(value_name = "NAME", help = "The built-in flow, as `cornercase flow list` names it")]
    pub(crate) name: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowSaveArgs {
    #[arg(
        value_name = "FILE",
        help = "The flow as JSON, the `flow` of an entry of `flow list --json`, or - for standard input"
    )]
    pub(crate) source: PathBuf,
    #[arg(long, value_enum, help = "Where the flow goes [default: where the --path file is, else yours]")]
    pub(crate) to: Option<ImportTo>,
    #[arg(
        long,
        value_name = "ID",
        help = "The project, for --to project and the flows its name may clash with [default: the one this runs in, else the shown one]"
    )]
    pub(crate) project: Option<u64>,
    #[arg(
        long,
        value_name = "FILE",
        help = "The file of the flow being edited, as `flow list --json` gives it: a new name renames it, --to moves it"
    )]
    pub(crate) path: Option<PathBuf>,
    #[arg(long, help = "List the flow's problems and save nothing")]
    pub(crate) check: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct FlowDeleteArgs {
    #[arg(
        value_name = "NAME",
        required_unless_present = "path",
        conflicts_with = "path",
        help = "The flow, as `cornercase flow list` names it"
    )]
    pub(crate) name: Option<String>,
    #[arg(
        long = "in",
        value_enum,
        help = "Only the flow of yours or the project's [default: either, when only one has it]"
    )]
    pub(crate) place: Option<ImportTo>,
    #[arg(long, value_name = "ID", help = "The project whose flows to look in [default: the one this runs in]")]
    pub(crate) project: Option<u64>,
    #[arg(
        long,
        value_name = "FILE",
        help = "The flow's file, also one that cannot be read, as `flow list --json` gives it"
    )]
    pub(crate) path: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) print: Print,
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
        help = "The worktree's branch, made when missing [default: the issue's with --issue, else from the task's first words, made unique]"
    )]
    pub(crate) worktree: Option<String>,
    #[arg(long, value_name = "ID", help = "The project [default: the one this runs in, else the shown one]")]
    pub(crate) project: Option<u64>,
    #[command(flatten)]
    pub(crate) issue: issue::IssueArg,
    #[command(flatten)]
    pub(crate) profile: accounts::ProfileArg,
    #[command(flatten)]
    pub(crate) create: Create,
}
