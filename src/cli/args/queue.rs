use clap::{ArgGroup, Args, Subcommand};

use super::{FreshArg, Print};
use crate::cli::help::QUEUE_FRESH_HELP;

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
    #[command(about = "Remove an entry and print the number that restore takes to put it back")]
    Remove {
        id: u64,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Start a pending, missed or failed entry now, or a routine once more")]
    Run { id: u64 },
    #[command(
        about = "Show what a started entry's agent did: its last message, branch and pull request; --conversation \
                 adds the whole conversation"
    )]
    Show {
        id: u64,
        #[arg(long, help = "Also print the whole conversation, prompts and replies, while the agent keeps its record")]
        conversation: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Stop a routine from firing until it is resumed")]
    Pause { id: u64 },
    #[command(about = "Let a paused routine fire again, from the next occurrence after now")]
    Resume { id: u64 },
    #[command(about = "Remove every started entry and print the number that restore takes to put them back")]
    ClearStarted(Print),
    #[command(about = "Put back what remove or clear-started removed")]
    Restore {
        #[arg(value_name = "UNDO", help = "The number remove or clear-started printed")]
        undo: u64,
    },
}

#[derive(Debug, Args)]
#[command(
    group(ArgGroup::new("branch_from").args(["fresh", "no_fresh"]).conflicts_with_all(["every", "workspace"])),
    mut_arg("fresh", |arg| arg.help(QUEUE_FRESH_HELP))
)]
pub(crate) struct QueueAdd {
    #[arg(
        long,
        help = "Local time: 14:30, tomorrow 9:00, 2026-10-08 09:00, in 2h, in 1w, claude reset or codex reset"
    )]
    pub(crate) at: Option<String>,
    #[arg(
        long,
        conflicts_with_all = ["at", "issue", "workspace", "worktree"],
        help = "Make a routine that repeats: 30m, 2h, 3d, 2w, 'day 09:00', 'weekday 09:00' or 'monday 09:00'. At least \
                30 minutes apart; each run gets its own worktree from the up-to-date default branch"
    )]
    pub(crate) every: Option<String>,
    #[arg(long, conflicts_with_all = ["agent", "mode"], help = "Run this flow in each run of a routine; the prompt is its task")]
    pub(crate) flow: Option<String>,
    #[arg(long, help = "Remove a routine run's workspace when it finishes, if nothing would be lost")]
    pub(crate) cleanup: bool,
    #[arg(long, conflicts_with = "cleanup", help = "Keep a routine run's workspace (the default)")]
    pub(crate) no_cleanup: bool,
    #[arg(long, help = "Agent name, or auto to use the current default or running agent")]
    pub(crate) agent: Option<String>,
    #[arg(
        long,
        help = "One of the agent's modes, as settings → agents lists them, or default for no mode arguments \
                [default: the one chosen there; edit keeps the entry's]"
    )]
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
    #[command(flatten)]
    pub(crate) fresh: FreshArg,
    #[arg(long, help = "Entry and tab name")]
    pub(crate) name: Option<String>,
    #[arg(
        long = "account",
        value_name = "AGENT=ACCOUNT",
        value_parser = super::accounts::choice,
        help = "Start that agent with this account; AGENT= is its system login. Give it once per agent [default: the \
                accounts chosen when it starts]"
    )]
    pub(crate) accounts: Vec<(String, String)>,
    #[command(flatten)]
    pub(crate) print: Print,
}
