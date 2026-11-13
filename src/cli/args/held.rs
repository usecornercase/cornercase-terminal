use clap::Args;

use super::{BootArg, Print, Target};

#[derive(Debug, Args)]
pub(crate) struct HeldArgs {
    #[command(flatten)]
    pub(crate) target: Target,
    #[command(flatten)]
    pub(crate) button: HeldButton,
    #[command(flatten)]
    pub(crate) print: Print,
    #[command(flatten)]
    pub(crate) boot: BootArg,
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(crate) struct HeldButton {
    #[arg(
        long,
        help = "Send the first held message at once, still after an earlier Enter; over the person's draft it is pasted without its Enter, which is left to them"
    )]
    pub(crate) send_now: bool,
    #[arg(long, help = "Only say the person saw it: the messages held so far still go after their prompt")]
    pub(crate) wait_turn: bool,
}
