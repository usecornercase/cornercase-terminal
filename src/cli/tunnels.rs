use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::args::tunnels::{FailedArg, PortArg, TunnelAction, TunnelArgs, TunnelsArgs};
use super::{print_json, print_out};
use crate::client::{answer, ask};
use crate::control::{Command, ListTunnels, TunnelFailed, TunnelInfo, TunnelList, TunnelPort};
use crate::error::Result;

mod hold;
mod orders;

const NO_WINDOW: &str = "no window holds tunnels for this server: open one with `cornercase remote` from the computer that should listen, or take the orders with `cornercase --remote DESTINATION tunnel orders`\n";

pub(super) fn run_list(args: &TunnelsArgs) -> Result<bool> {
    let value = ask("tunnels", Command::Tunnels(ListTunnels::default()))?;
    let list: TunnelList = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&list))?;
    }
    Ok(true)
}

pub(super) fn run(args: &TunnelArgs) -> Result<bool> {
    let (name, port, print, command): (_, _, _, fn(TunnelPort) -> Command) = match &args.action {
        TunnelAction::Open(arg) => ("tunnel-open", arg.port, &arg.print, Command::TunnelOpen),
        TunnelAction::Close(arg) => ("tunnel-close", arg.port, &arg.print, Command::TunnelClose),
        TunnelAction::Forget(arg) => ("tunnel-forget", arg.port, &arg.print, Command::TunnelForget),
        TunnelAction::Hold(arg) => return hold::run(arg.port),
        TunnelAction::Orders => return orders::run(),
        TunnelAction::Failed(arg) => return failed(arg),
    };
    let value = ask(name, command(TunnelPort { port }))?;
    let list: TunnelList = answer(value.clone())?;
    if print.json {
        print_json(&value)?;
    } else {
        print_out(&done_text(&args.action, &list))?;
    }
    Ok(true)
}

fn failed(arg: &FailedArg) -> Result<bool> {
    let command = Command::TunnelFailed(TunnelFailed { port: arg.port, error: arg.reason.clone() });
    let value = ask("tunnel-failed", command)?;
    if arg.print.json {
        print_json(&value)?;
    } else {
        print_out(&format!("reported that port {} could not be tunnelled\n", arg.port))?;
    }
    Ok(true)
}

fn done_text(action: &TunnelAction, list: &TunnelList) -> String {
    match action {
        TunnelAction::Open(PortArg { port, .. }) => opened_text(*port, list),
        TunnelAction::Close(PortArg { port, .. }) => format!("closed the tunnel of port {port}\n"),
        TunnelAction::Forget(PortArg { port, .. }) => format!("forgot the tunnel of port {port}\n"),
        TunnelAction::Hold(_) | TunnelAction::Orders | TunnelAction::Failed(_) => String::new(),
    }
}

fn opened_text(port: u16, list: &TunnelList) -> String {
    let mut text = String::new();
    for tunnel in &list.tunnels {
        if tunnel.state == "up" {
            let local = tunnel.local.unwrap_or(port);
            let _ = write!(text, "port {port} is tunnelled to localhost:{local}");
        } else {
            let _ = write!(text, "port {port} is not tunnelled: {}", tunnel.error.as_deref().unwrap_or("unknown"));
        }
        text.push_str(&holder_suffix(tunnel, list.windows > 1));
        text.push('\n');
    }
    text
}

pub(crate) fn text(list: &TunnelList) -> String {
    if list.tunnels.is_empty() {
        return if list.windows == 0 && list.takers == 0 { NO_WINDOW.into() } else { "no tunnels\n".into() };
    }
    let holders = list.tunnels.iter().map(|tunnel| tunnel.holder).collect::<BTreeSet<u64>>().len();
    let several = list.windows > 1 || holders > 1;
    list.tunnels.iter().map(|tunnel| line(tunnel, several)).collect()
}

fn line(tunnel: &TunnelInfo, several: bool) -> String {
    let mut text = format!("{}", tunnel.port);
    match (tunnel.state.as_str(), tunnel.local) {
        ("up", Some(local)) => {
            let _ = write!(text, " → localhost:{local}  up");
        }
        ("down", _) => {
            let _ = write!(text, "  down: {}", tunnel.error.as_deref().unwrap_or("unknown"));
        }
        (state, _) => {
            let _ = write!(text, "  {state}");
        }
    }
    text.push_str(&holder_suffix(tunnel, several));
    text.push('\n');
    text
}

fn holder_suffix(tunnel: &TunnelInfo, several: bool) -> String {
    if tunnel.kind == "hold" {
        format!(" (hold {})", tunnel.holder)
    } else if several {
        format!(" (window {})", tunnel.holder)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests;
