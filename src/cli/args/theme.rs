use clap::Args;

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct ThemeArgs {
    #[arg(
        value_name = "FILE",
        conflicts_with = "clear",
        help = "Colours as JSON, or - for standard input; without it, say where the colours come from"
    )]
    pub(crate) file: Option<String>,
    #[arg(long, help = "Give the terminals the colours of the first window again, or the defaults")]
    pub(crate) clear: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}
