use clap::Args;

use super::{BootArg, Print, Target};

#[derive(Debug, Args)]
pub(crate) struct ContextArgs {
    #[command(flatten)]
    pub(crate) target: Target,
    #[arg(long, help = "Print the last answer with its age, without asking the agent again")]
    pub(crate) cached: bool,
    #[command(flatten)]
    pub(crate) print: Print,
    #[command(flatten)]
    pub(crate) boot: BootArg,
}
