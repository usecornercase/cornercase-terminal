use std::io::Write;

use serde_json::Value;

use crate::cli::quietly;
use crate::client;
use crate::control::{Command, TunnelOrders};
use crate::error::{Error, Result};

const NEEDS_REMOTE: &str = "`tunnel orders` needs --remote DESTINATION: it takes the tunnels that machine's server is \
    asked for, to hold them from this computer";

pub(super) fn run() -> Result<bool> {
    client::own_connection()?;
    client::used_remote().ok_or_else(|| Error::WrongUsage(NEEDS_REMOTE.into()))?;
    client::use_tunnel_orders()?;
    let mut out = std::io::stdout().lock();
    client::follow_all("tunnel-orders", Command::TunnelOrders(TunnelOrders {}), |value| write_line(&mut out, &value))?;
    Ok(true)
}

fn write_line(out: &mut impl Write, value: &Value) -> Result<bool> {
    quietly(writeln!(out, "{value}").and_then(|()| out.flush()))
}

#[cfg(test)]
mod tests;
