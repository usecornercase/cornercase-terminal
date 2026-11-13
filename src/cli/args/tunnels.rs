use clap::{Args, Subcommand};

use super::Print;

#[derive(Debug, Args)]
pub(crate) struct TunnelsArgs {
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct TunnelArgs {
    #[command(subcommand)]
    pub(crate) action: TunnelAction,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TunnelAction {
    #[command(about = "Forward a port of this server's machine to the computer of the window attached to it")]
    Open(PortArg),
    #[command(about = "Close the tunnel of a port")]
    Close(EndArg),
    #[command(about = "Forget a tunnel: close it and stop windows from opening it again")]
    Forget(EndArg),
    #[command(
        about = "Hold a tunnel to the machine of --remote with no window, until this command ends (--reverse: from it to this computer)"
    )]
    Hold(HoldArg),
    #[command(
        about = "Take the tunnel orders of the server of --remote for a program with no window, until this command ends"
    )]
    Orders,
    #[command(about = "Tell the server that the tunnel of a port it asked for cannot be held from this computer")]
    Failed(FailedArg),
}

#[derive(Debug, Args)]
pub(crate) struct FailedArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port that was asked for, on this server's machine")]
    pub(crate) port: u16,
    #[arg(value_name = "REASON", help = "Why it cannot be held, as one line: `tunnel open` fails with it")]
    pub(crate) reason: String,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct HoldArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), required_unless_present = "reverse", conflicts_with = "reverse", help = "The port to tunnel, on the machine of --remote")]
    pub(crate) port: Option<u16>,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..), conflicts_with = "reverse", help = "The port of this computer to listen on when PORT is taken here, such as the one the hold had before (any free one when N is taken too)")]
    pub(crate) local: Option<u16>,
    #[arg(long, value_name = "LOCAL", value_parser = clap::value_parser!(u16).range(1..), help = "Hold a reverse tunnel: make port LOCAL of this computer reachable on 127.0.0.1 of the machine of --remote")]
    pub(crate) reverse: Option<u16>,
    #[arg(long, value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), requires = "reverse", help = "With --reverse, the port to listen on there; the hold fails when it is taken (without it, the machine's sshd picks a free one)")]
    pub(crate) there: Option<u16>,
}

#[derive(Debug, Args)]
pub(crate) struct EndArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port of the tunnel, on this server's machine (for --reverse, the port it listens on there)")]
    pub(crate) port: u16,
    #[arg(
        long,
        help = "Act on the reverse tunnel listening on PORT (`tunnel hold --reverse`), not on the tunnel of PORT"
    )]
    pub(crate) reverse: bool,
    #[command(flatten)]
    pub(crate) print: Print,
}

#[derive(Debug, Args)]
pub(crate) struct PortArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port to tunnel, on this server's machine")]
    pub(crate) port: u16,
    #[command(flatten)]
    pub(crate) print: Print,
}
