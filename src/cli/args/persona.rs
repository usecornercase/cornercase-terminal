use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::Print;
use crate::cli::help::{PERSONA_EDIT_HELP, PERSONA_IMPORT_HELP, PERSONA_NEW_HELP};

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
    #[command(about = "Print one of your personas, its front matter and its instructions")]
    Show(PersonaIdArgs),
    #[command(about = "Make a persona of your own from a name and its instructions", after_help = PERSONA_NEW_HELP)]
    New(PersonaNewArgs),
    #[command(about = "Change one of your personas: its name, description, emoji or instructions", after_help = PERSONA_EDIT_HELP)]
    Edit(PersonaEditArgs),
    #[command(about = "Delete one of your personas; a running flow keeps its own copy")]
    Remove(PersonaIdArgs),
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
pub(crate) struct PersonaIdArgs {
    #[arg(value_name = "ID", help = "The persona's id, as `persona list` prints it")]
    pub(crate) id: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PersonaNewArgs {
    #[arg(value_name = "INSTRUCTIONS", help = "A Markdown file with the instructions, or - for standard input")]
    pub(crate) instructions: PathBuf,
    #[arg(long, help = "Its name, as the window and `persona list` show it")]
    pub(crate) name: String,
    #[arg(long, help = "One line on what it is for")]
    pub(crate) description: Option<String>,
    #[arg(long, help = "An emoji shown before its name")]
    pub(crate) emoji: Option<String>,
    #[arg(long, value_name = "ID", help = "Its id [default: from the name]")]
    pub(crate) id: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PersonaEditArgs {
    #[arg(value_name = "ID", help = "The persona's id, as `persona list` prints it")]
    pub(crate) id: String,
    #[arg(long, help = "A new name")]
    pub(crate) name: Option<String>,
    #[arg(long, help = "A new description, or '' for none")]
    pub(crate) description: Option<String>,
    #[arg(long, help = "A new emoji, or '' for none")]
    pub(crate) emoji: Option<String>,
    #[arg(long, value_name = "FILE", help = "New instructions, from a Markdown file or - for standard input")]
    pub(crate) instructions: Option<PathBuf>,
    #[command(flatten)]
    pub(crate) print: Print,
}
