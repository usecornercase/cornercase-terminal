use clap::Args;

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct RecapArgs {
    #[arg(value_name = "RANGE", default_value = "now", value_parser = ["now", "yesterday", "week"], help = "What to sum up: now, yesterday or week")]
    pub(crate) range: String,
    #[arg(long, help = "Write it again, even if the last one is still current")]
    pub(crate) fresh: bool,
    #[arg(long, help = "Print the dossier the agent would read, without running an agent")]
    pub(crate) dossier: bool,
    #[arg(long, value_name = "AGENT", help = "The agent that writes it (claude or codex), for this run only")]
    pub(crate) agent: Option<String>,
    #[arg(long, value_name = "NAME", help = "The agent's account, for this run only")]
    pub(crate) account: Option<String>,
    #[arg(long, value_name = "MODEL", help = "The model, for this run only")]
    pub(crate) model: Option<String>,
    #[arg(long, value_name = "LEVEL", help = "The effort, for this run only")]
    pub(crate) effort: Option<String>,
    #[arg(long, value_name = "LANGUAGE", help = "The language it is written in, for this run only")]
    pub(crate) language: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}
