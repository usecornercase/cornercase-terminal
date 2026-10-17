use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    #[command(subcommand)]
    pub(crate) action: Option<ConfigAction>,
    #[arg(long, global = true, help = "Print JSON instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
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
    },
    #[command(about = "Check and save a tracker's token or a bot's, read from standard input")]
    Secret {
        #[arg(value_name = "KEY", help = "The connection, such as linear or telegram")]
        key: String,
    },
    #[command(about = "Remove a token, a bot, a paired chat or the paired browsers")]
    Remove {
        #[arg(value_name = "KEY", help = "The connection, such as linear, telegram.chat or browsers")]
        key: String,
    },
}
