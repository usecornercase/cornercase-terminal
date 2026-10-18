use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::{HERE_PANE, TODO_SEND_HELP};

#[derive(Debug, Args)]
pub(crate) struct TodoArgs {
    #[command(subcommand)]
    pub(crate) action: TodoAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TodoAction {
    #[command(about = "Add an item and print its id")]
    Add {
        #[arg(required = true, value_name = "TEXT", help = "What to do")]
        text: Vec<String>,
    },
    #[command(about = "List the items with their ids, pending ones first")]
    List(Print),
    #[command(about = "Check an item off, or back on")]
    Done {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
    },
    #[command(about = "Send an item's text to an agent, like `send --enter`, and check it off", after_help = TODO_SEND_HELP)]
    Send {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
        #[arg(long, value_name = "ID", help = HERE_PANE)]
        pane: Option<u64>,
        #[arg(long, value_name = "ID", conflicts_with = "pane", help = "A tab: its agent's pane, else its active one")]
        tab: Option<u64>,
        #[arg(
            long,
            help = "Send even when the agent shows a dialog, a panel or its shell mode instead of its input box, or \
                    its feedback survey"
        )]
        force: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Remove an item")]
    Rm {
        #[arg(value_name = "ID", help = "The item")]
        id: u64,
    },
}
