use clap::{Args, Subcommand};

use super::Print;
use super::changes::Where;

#[derive(Debug, Args)]
pub(crate) struct FilesArgs {
    #[command(subcommand)]
    pub(crate) action: FilesAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum FilesAction {
    #[command(about = "List the entries of folders, with their kind, size, modification time and git mark")]
    List {
        #[arg(
            value_name = "FOLDER",
            help = "A folder, relative to the workspace, absolute or starting with ~/ [default: the workspace's root]"
        )]
        folders: Vec<String>,
        #[command(flatten)]
        place: Where,
        #[arg(long, help = "Keep running and print the folders as one JSON line each time they change")]
        follow: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Print a file's text, or what kind of file it is")]
    Read {
        #[arg(value_name = "PATH", help = "The file, relative to the workspace, absolute or starting with ~/")]
        path: String,
        #[command(flatten)]
        place: Where,
        #[arg(long, help = "Keep running and print the file as one JSON line each time it changes")]
        follow: bool,
        #[command(flatten)]
        print: Print,
    },
    #[command(about = "Search the names or the text of the files of a workspace")]
    Search {
        #[arg(value_name = "QUERY", help = "What to look for")]
        query: String,
        #[arg(long, conflicts_with = "text", help = "Search file names, fuzzy and ranked (the panel's name mode)")]
        names: bool,
        #[arg(long, help = "Search the text of the files (the default)")]
        text: bool,
        #[command(flatten)]
        place: Where,
        #[command(flatten)]
        print: Print,
    },
}
