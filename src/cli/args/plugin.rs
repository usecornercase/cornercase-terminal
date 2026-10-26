use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::{LogsArgs, Print, Which};
use crate::plugins::install::is_commit;

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
    Install(InstallArgs),
    #[command(about = "Fetch an installed plugin again and, if it changed, show what it runs and ask before updating")]
    Update(UpdatePluginArgs),
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
    #[command(
        about = "Show the plugins as Settings › Plugins does, including what each service is doing; asks the server"
    )]
    Status(Print),
    #[command(about = "List the actions plugins add to the menus, for one kind of row or every one; asks the server")]
    Actions {
        #[arg(
            long,
            value_name = "KIND",
            value_parser = ["project", "workspace", "tab", "pane", "button"],
            help = "Only the actions offered for a project, workspace, tab or pane, or on the plugins' buttons"
        )]
        context: Option<String>,
        #[command(flatten)]
        print: Print,
    },
    #[command(
        about = "Run a plugin's action on a project, workspace, tab or pane as its menu entry does, or a button's on \
                 what the window shows; asks the server"
    )]
    Run(RunArgs),
    #[command(about = "Close the plugin popup that is open; asks the server")]
    PopupClose,
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

#[derive(Debug, Args)]
#[command(mut_group("Which", |group| group.required(false)))]
pub(crate) struct RunArgs {
    #[arg(value_name = "PLUGIN", help = "The plugin's id")]
    pub(crate) plugin: String,
    #[arg(value_name = "ACTION", help = "The action's id, as `plugin actions` lists it")]
    pub(crate) action: String,
    #[command(flatten)]
    pub(crate) which: Which,
    #[arg(
        long,
        value_name = "N",
        help = "The revision `plugin actions --json` printed; refused once the plugins changed. Left out, the current one"
    )]
    pub(crate) revision: Option<u32>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct InstallArgs {
    #[arg(
        value_name = "SOURCE",
        help = "owner/repo[/folder], an https:// git address or a git folder (with //folder for a plugin in a \
                folder of it), then @branch, @tag or @commit to pin one"
    )]
    pub(crate) source: String,
    #[arg(
        short,
        long,
        conflicts_with = "preview",
        help = "Install without asking; needed when there is no terminal to ask in"
    )]
    pub(crate) yes: bool,
    #[arg(long, help = "Fetch it and print what installing it would run, then exit without asking or installing")]
    pub(crate) preview: bool,
    #[arg(
        long,
        requires = "preview",
        help = "With --preview, print it as JSON, for a program that shows it and asks itself"
    )]
    pub(crate) json: bool,
    #[arg(
        long,
        value_name = "COMMIT",
        value_parser = commit,
        conflicts_with = "preview",
        help = "Install only if SOURCE still gives this commit, the one --preview printed; unlike SOURCE@COMMIT, \
                updates keep following its branch or tag"
    )]
    pub(crate) commit: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct UpdatePluginArgs {
    #[arg(value_name = "ID", help = "The plugin to update; every installed one when left out")]
    pub(crate) id: Option<String>,
    #[arg(
        short,
        long,
        conflicts_with = "preview",
        help = "Update without asking; needed when there is no terminal to ask in"
    )]
    pub(crate) yes: bool,
    #[arg(
        long,
        help = "Fetch it and print what updating would run, or that it is up to date, then exit without asking or updating"
    )]
    pub(crate) preview: bool,
    #[arg(
        long,
        requires_all = ["preview", "id"],
        help = "With --preview and an ID, print it as JSON, for a program that shows it and asks itself"
    )]
    pub(crate) json: bool,
    #[arg(
        long,
        value_name = "COMMIT",
        value_parser = commit,
        requires = "id",
        conflicts_with = "preview",
        help = "Update only to this commit, the one --preview printed; refused once the plugin gives another"
    )]
    pub(crate) commit: Option<String>,
}

fn commit(text: &str) -> Result<String, String> {
    let commit = text.trim().to_ascii_lowercase();
    if is_commit(&commit) {
        Ok(commit)
    } else {
        Err("use the whole commit, 40 hexadecimal digits, as --preview prints it".into())
    }
}
