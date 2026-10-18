use clap::{Args, Subcommand};

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct QueueArgs {
    #[command(subcommand)]
    pub(crate) action: QueueAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum QueueAction {
    #[command(about = "Schedule a prompt or issue and print the entry's persistent id")]
    Add(QueueAdd),
    #[command(about = "Edit an entry that has not started")]
    Edit {
        id: u64,
        #[command(flatten)]
        add: QueueAdd,
    },
    #[command(about = "List scheduled, started, failed and missed entries")]
    List(Print),
    #[command(about = "Cancel or remove an entry")]
    Remove { id: u64 },
    #[command(about = "Start a pending, missed or failed entry now")]
    Run { id: u64 },
}

#[derive(Debug, Args)]
pub(crate) struct QueueAdd {
    #[arg(long, help = "Local time: 14:30, tomorrow 9:00, 2026-10-08 09:00, in 2h, claude reset or codex reset")]
    pub(crate) at: Option<String>,
    #[arg(long, help = "Agent name, or auto to use the current default or running agent")]
    pub(crate) agent: Option<String>,
    #[arg(long, help = "One of the agent's configured modes")]
    pub(crate) mode: Option<String>,
    #[arg(long, conflicts_with = "issue", help = "Prompt to submit when the entry starts")]
    pub(crate) prompt: Option<String>,
    #[arg(long, help = "Issue URL or key; its prompt uses the configured template")]
    pub(crate) issue: Option<String>,
    #[arg(long, requires = "issue", value_parser = super::issue::SOURCES, help = "Tracker for an ambiguous issue key or a self-hosted URL")]
    pub(crate) source: Option<String>,
    #[arg(long, conflicts_with = "workspace", help = "Project id for a repository root or new worktree")]
    pub(crate) project: Option<u64>,
    #[arg(long, conflicts_with = "worktree", help = "Workspace id for a new tab")]
    pub(crate) workspace: Option<u64>,
    #[arg(long, help = "Branch for a worktree, made when missing")]
    pub(crate) worktree: Option<String>,
    #[arg(long, help = "Entry and tab name")]
    pub(crate) name: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}
