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
}

#[derive(Debug, Args)]
pub(crate) struct PortArg {
    #[arg(value_name = "PORT", value_parser = clap::value_parser!(u16).range(1..), help = "The port to tunnel, on this server's machine")]
    pub(crate) port: u16,
    #[command(flatten)]
    pub(crate) print: Print,
}
