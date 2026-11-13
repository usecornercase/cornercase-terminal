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
    Close(PortArg),
    #[command(about = "Forget a tunnel: close it and stop windows from opening it again")]
    Forget(PortArg),
    #[command(about = "Hold a tunnel to the machine of --remote with no window, until this command ends")]
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
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port to tunnel, on the machine of --remote")]
    pub(crate) port: u16,
}

#[derive(Debug, Args)]
pub(crate) struct PortArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port to tunnel, on this server's machine")]
    pub(crate) port: u16,
    #[command(flatten)]
    pub(crate) print: Print,
}
