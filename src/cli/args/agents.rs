use clap::{Args, Subcommand};

use super::Print;
use super::changes::Where;

#[derive(Debug, Args)]
pub(crate) struct AgentsArgs {
    #[command(subcommand)]
    pub(crate) action: AgentsAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgentsAction {
    #[command(about = "Close the tabs of a workspace that run agents, to reopen them later")]
    Close {
        #[command(flatten)]
        place: Where,
        #[arg(long, help = "Say what would close and close nothing")]
        check: bool,
        #[arg(long, help = "Close even agents that are working or cannot resume, and the other panes of their tabs")]
        force: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Reopen the closed agents of a workspace, each in its own conversation")]
    Reopen {
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Forget the closed agents of a workspace, after asking")]
    Forget {
        #[command(flatten)]
        place: Where,
        #[arg(short, long, help = "Forget without asking")]
        yes: bool,
        #[command(flatten)]
        print: Print,
    },
}
