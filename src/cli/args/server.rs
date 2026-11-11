use clap::Args;

use super::{Print, seconds};
use crate::cli::help::{COMMAND_THERE, RESTART_TIMEOUT, WHEN_IDLE};

#[derive(Debug, Args)]
#[command(override_usage = "cornercase remote [OPTIONS] <DESTINATION>\n       cornercase remote --known [--json]")]
pub(crate) struct RemoteArgs {
    #[arg(
        value_name = "DESTINATION",
        required_unless_present = "known",
        help = "Where ssh connects: a host from ~/.ssh/config, or user@host"
    )]
    pub(crate) destination: Option<String>,
    #[arg(long, value_name = "COMMAND", help = COMMAND_THERE)]
    pub(crate) command: Option<String>,
    #[arg(
        long,
        conflicts_with_all = ["destination", "command"],
        help = "Print the machines a window on this machine reached, as its machine button lists them, and open no \
                window; it reads this machine's list, so it needs no server or ssh"
    )]
    pub(crate) known: bool,
    #[arg(long, conflicts_with_all = ["destination", "command"], help = "With --known, print JSON instead of text")]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LogsArgs {
    #[arg(short = 'n', long, value_name = "N", default_value_t = 200, help = "How many of the last lines to print")]
    pub(crate) lines: usize,
    #[arg(short, long, help = "Keep printing what the server writes, until interrupted")]
    pub(crate) follow: bool,
    #[arg(long, conflicts_with_all = ["lines", "follow"], help = "Print only where the log is")]
    pub(crate) path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct StartServerArgs {
    #[arg(
        long,
        value_name = "N",
        help = "Columns of the terminals until a window attaches, 90 to 1000 [default: 160]"
    )]
    pub(crate) cols: Option<u16>,
    #[arg(long, value_name = "N", help = "Rows of the terminals until a window attaches, 10 to 500 [default: 48]")]
    pub(crate) rows: Option<u16>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Colours programs in the terminals see: a JSON file, or - for standard input (see `cornercase theme`)"
    )]
    pub(crate) theme: Option<String>,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct KillServerArgs {
    #[arg(short, long, help = "Stop the server without asking, even if this terminal is one of its own")]
    pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RestartArgs {
    #[arg(short, long, help = "Restart the server without asking")]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) when: WhenArgs,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
    #[arg(long, conflicts_with = "when_idle", help = "Only say whether a newer version is out")]
    pub(crate) check: bool,
    #[arg(short, long, help = "Restart the server without asking")]
    pub(crate) yes: bool,
    #[command(flatten)]
    pub(crate) when: WhenArgs,
}

#[derive(Debug, Args)]
pub(crate) struct WhenArgs {
    #[arg(long, help = WHEN_IDLE)]
    pub(crate) when_idle: bool,
    #[arg(long, value_name = "SECONDS", value_parser = seconds, requires = "when_idle", help = RESTART_TIMEOUT)]
    pub(crate) timeout: Option<f64>,
}
