use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::{MACHINES_ADD_HELP, MACHINES_UPDATE_HELP};

const ICON: &str = "Its icon, by glyph or name: ☁ server, ▭ laptop, ▣ desktop, ◫ mini, ⌂ home server, ◆ other";
const NAME: &str = "The machine's name, as `cornercase machines` lists it";

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct MachinesArgs {
    #[command(subcommand)]
    pub(crate) action: Option<MachinesAction>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MachinesAction {
    #[command(about = "Add a machine: a name, its ssh destination, an icon and a folder", after_help = MACHINES_ADD_HELP)]
    Add(AddArgs),
    #[command(about = "Change a machine's name, destination, icon or folder", after_help = MACHINES_ADD_HELP)]
    Edit(EditArgs),
    #[command(about = "Remove a machine and end its link; its server keeps running there")]
    Remove(NameArgs),
    #[command(about = "Install this cornercase's release on an older machine", after_help = MACHINES_UPDATE_HELP)]
    Update(NameArgs),
    #[command(about = "Try the links again now: one machine's, or every machine's")]
    Retry(RetryArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AddArgs {
    #[arg(value_name = "NAME", help = "Its name: letters, digits, `.`, `_` and `-`, at most 20")]
    pub(crate) name: String,
    #[arg(value_name = "DESTINATION", help = "Where ssh reaches it: user@host, or a host of ~/.ssh/config")]
    pub(crate) destination: String,
    #[arg(long, value_name = "ICON", help = ICON)]
    pub(crate) icon: Option<String>,
    #[arg(long, value_name = "FOLDER", help = "Its folder for clones there [default: ~/code]")]
    pub(crate) folder: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct EditArgs {
    #[arg(value_name = "NAME", help = NAME)]
    pub(crate) name: String,
    #[arg(long = "name", value_name = "NEW_NAME", help = "Its new name")]
    pub(crate) new_name: Option<String>,
    #[arg(long, value_name = "DESTINATION", help = "Its new ssh destination; the link is made again")]
    pub(crate) destination: Option<String>,
    #[arg(long, value_name = "ICON", help = ICON)]
    pub(crate) icon: Option<String>,
    #[arg(long, value_name = "FOLDER", help = "Its new folder for clones there")]
    pub(crate) folder: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct NameArgs {
    #[arg(value_name = "NAME", help = NAME)]
    pub(crate) name: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct RetryArgs {
    #[arg(value_name = "NAME", help = "Only this machine [default: every machine]")]
    pub(crate) name: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}
