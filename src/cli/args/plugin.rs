use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::{LogsArgs, Print};

#[derive(Debug, Args)]
pub(crate) struct PluginArgs {
    #[command(subcommand)]
    pub(crate) action: PluginAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginAction {
    #[command(about = "Add the plugin in a folder of yours, turned off; for writing one")]
    Link {
        #[arg(value_name = "FOLDER", help = "The folder holding cornercase-plugin.toml")]
        folder: PathBuf,
    },
    #[command(
        about = "Install a plugin from GitHub, a git address or a folder, turned off, after showing what it runs"
    )]
    Install {
        #[arg(
            value_name = "SOURCE",
            help = "owner/repo[/folder], an https:// git address or a git folder (with //folder for a plugin in a \
                    folder of it), then @branch, @tag or @commit to pin one"
        )]
        source: String,
        #[arg(short, long, help = "Install without asking; needed when there is no terminal to ask in")]
        yes: bool,
    },
    #[command(about = "Fetch an installed plugin again and, if it changed, show what it runs and ask before updating")]
    Update {
        #[arg(value_name = "ID", help = "The plugin to update; every installed one when left out")]
        id: Option<String>,
        #[arg(short, long, help = "Update without asking; needed when there is no terminal to ask in")]
        yes: bool,
    },
    #[command(about = "Stop a plugin and delete its installed copy; a linked plugin's folder stays")]
    Remove {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Forget a linked plugin; its folder stays as it is")]
    Unlink {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "List the plugins, whether they are on, and what they run")]
    List(Print),
    #[command(about = "Turn a plugin on: it runs as you, with access to every terminal")]
    Enable {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Turn a plugin off, stopping what it runs")]
    Disable {
        #[arg(value_name = "ID")]
        id: String,
    },
    #[command(about = "Print what a plugin's commands wrote, and when cornercase started and stopped them")]
    Logs {
        #[arg(value_name = "ID")]
        id: String,
        #[command(flatten)]
        logs: LogsArgs,
    },
}
