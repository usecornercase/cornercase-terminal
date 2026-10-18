use clap::Args;

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct UsageArgs {
    #[arg(long, help = "Print the last answers with their age, without asking the agents again")]
    pub(crate) cached: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}
