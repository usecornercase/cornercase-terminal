use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::PERSONA_IMPORT_HELP;

#[derive(Debug, Args)]
pub(crate) struct PersonaArgs {
    #[command(subcommand)]
    pub(crate) action: PersonaAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PersonaAction {
    #[command(
        about = "Import personas from a file, a folder, an address or a GitHub repository",
        after_help = PERSONA_IMPORT_HELP
    )]
    Import(PersonaImportArgs),
    #[command(about = "List your personas")]
    List(Print),
    #[command(about = "Delete one of your personas; a running flow keeps its own copy")]
    Remove(PersonaRemoveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PersonaImportArgs {
    #[arg(value_name = "SOURCE", help = "A .md file, a folder, -, an https:// address, or OWNER/REPO[/PATH]")]
    pub(crate) source: String,
    #[arg(long, help = "Overwrite the personas you already have with their ids")]
    pub(crate) replace: bool,
    #[arg(long, conflicts_with = "yes", help = "List what would be imported without asking or importing")]
    pub(crate) check: bool,
    #[arg(short = 'y', long, help = "Import without asking, though the personas come from the web")]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PersonaRemoveArgs {
    #[arg(value_name = "ID", help = "The persona's id, as `persona list` prints it")]
    pub(crate) id: String,
    #[command(flatten)]
    pub(crate) print: Print,
}
