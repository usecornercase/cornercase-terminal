use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::BootArg;
use crate::cli::help::CONFIG_PAIR_HELP;

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) action: Option<ConfigAction>,
    #[arg(long, global = true, help = "Print JSON instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
    #[command(about = "Find settings by the start of each word, as the search of the settings modal does")]
    Search {
        #[arg(required = true, value_name = "TEXT", help = "The words to look for; every word must match")]
        text: Vec<String>,
    },
    #[command(
        about = "List every model the flow editor offers for an agent, reading the agent's own list when it keeps one"
    )]
    Models {
        #[arg(value_name = "AGENT", help = "The agent, as the agent setting names it, such as opencode")]
        agent: String,
    },
    #[command(about = "Change settings: KEY VALUE pairs, checked together, all or none")]
    Set {
        #[arg(required = true, num_args = 2.., value_names = ["KEY", "VALUE"])]
        pairs: Vec<String>,
    },
    #[command(about = "Set the commands a project runs when it makes a worktree and before it removes one")]
    Project {
        #[arg(value_name = "FOLDER", help = "The project's folder", conflicts_with = "project")]
        path: Option<String>,
        #[arg(long, value_name = "ID", help = "The id of an open project, from `cornercase status`")]
        project: Option<u64>,
        #[arg(long, help = "The commands to run after a worktree is made; empty removes them")]
        setup: Option<String>,
        #[arg(long, help = "The commands to run before a worktree is removed; empty removes them")]
        teardown: Option<String>,
        #[arg(
            long,
            value_name = "NAMES",
            help = "The folders `worktrees free-builds` deletes, separated by spaces or commas; `default` restores the usual list; empty means none"
        )]
        builds: Option<String>,
        #[command(flatten)]
        boot: BootArg,
    },
    #[command(about = "Check and save a tracker's token or a bot's, read from standard input")]
    Secret {
        #[arg(value_name = "KEY", help = "The connection, such as linear or telegram")]
        key: String,
    },
    #[command(about = "Remove a token, a bot, a paired chat, the paired browsers or a paired phone app")]
    Remove {
        #[arg(value_name = "KEY", help = "The connection, such as linear, telegram.chat, browsers or app.ID")]
        key: String,
    },
    #[command(about = "Pair a phone: print the address its QR code holds, then wait for it", after_help = CONFIG_PAIR_HELP)]
    Pair {
        #[arg(value_name = "KEY", help = "What to pair: app, browsers, telegram.chat or discord.chat")]
        key: String,
        #[arg(long, help = "With app: print the addresses the phone app reaches now, then end")]
        routes: bool,
    },
    #[command(about = "Pick the server of the Discord forum, once the forum's setup waits for that")]
    ForumServer {
        #[arg(value_name = "ID", help = "The server's id, from `forum.servers` in `cornercase status --app`")]
        id: String,
    },
    #[command(about = "Write the settings to share with a colleague, without tokens or personal ones")]
    Export {
        #[arg(long, value_name = "FILE", help = "Write them to FILE instead of standard output")]
        to: Option<PathBuf>,
    },
    #[command(about = "Import settings a colleague shared, after showing what changes")]
    Import {
        #[arg(value_name = "FILE", help = "A file `config export` made, or - for standard input")]
        source: String,
        #[arg(
            long,
            value_delimiter = ',',
            value_name = "SECTIONS",
            help = "Only these sections: agents, issues, worktrees, interface, repositories"
        )]
        only: Vec<String>,
        #[arg(long, help = "Import without asking", conflicts_with = "preview")]
        yes: bool,
        #[arg(long, help = "Show what would change and exit, without asking or importing")]
        preview: bool,
    },
}

impl ConfigAction {
    pub(crate) fn boot(&self) -> Option<&BootArg> {
        match self {
            Self::Project { boot, .. } => Some(boot),
            _ => None,
        }
    }
}
