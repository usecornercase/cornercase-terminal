use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::args::tunnels::{EndArg, FailedArg, PortArg, TunnelAction, TunnelArgs, TunnelsArgs};
use super::{print_json, print_out};
use crate::client::{self, answer, ask};
use crate::control::{
    Command, Direction, ListTunnels, MachineTunnelInfo, TunnelFailed, TunnelInfo, TunnelList, TunnelPort,
};
use crate::error::Result;

mod hold;
mod orders;

const NO_WINDOW: &str = "no window holds tunnels for this server: open one with `cornercase remote` from the computer that should listen, or have a program there run `tunnel orders`\n";

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
    let (name, tunnel, print, command): (_, _, _, fn(TunnelPort) -> Command) = match &args.action {
        TunnelAction::Open(arg) => {
            let tunnel = TunnelPort { port: arg.port, direction: None, machine: arg.machine.clone() };
            ("tunnel-open", tunnel, &arg.print, Command::TunnelOpen)
        }
        TunnelAction::Close(arg) => ("tunnel-close", ending(arg), &arg.print, Command::TunnelClose),
        TunnelAction::Forget(arg) => ("tunnel-forget", ending(arg), &arg.print, Command::TunnelForget),
        TunnelAction::Hold(arg) => return hold::run(arg),
        TunnelAction::Orders => return orders::run(),
        TunnelAction::Failed(arg) => return failed(arg),
    };
    if tunnel.direction == Some(Direction::Reverse) {
        client::use_tunnel_reverse()?;
    }
    if tunnel.machine.is_some() {
        client::use_tunnel_machine()?;
    }
    let value = ask(name, command(tunnel))?;
    let list: TunnelList = answer(value.clone())?;
    if print.json {
        print_json(&value)?;
    } else {
        print_out(&done_text(&args.action, &list))?;
    }
    Ok(true)
}

fn ending(arg: &EndArg) -> TunnelPort {
    TunnelPort { port: arg.port, direction: arg.reverse.then_some(Direction::Reverse), machine: arg.machine.clone() }
}

fn failed(arg: &FailedArg) -> Result<bool> {
    let command = Command::TunnelFailed(TunnelFailed { port: arg.port, error: arg.reason.clone() });
    let value = ask("tunnel-failed", command)?;
    if arg.print.json {
        print_json(&value)?;
    } else {
        print_out(&failed_text(arg.port))?;
    }
    Ok(true)
}

fn failed_text(port: u16) -> String {
    format!("reported that port {port} could not be tunnelled\n")
}

fn done_text(action: &TunnelAction, list: &TunnelList) -> String {
    match action {
        TunnelAction::Open(PortArg { port, machine: Some(machine), .. }) => machine_opened_text(*port, machine, list),
        TunnelAction::Open(PortArg { port, .. }) => opened_text(*port, list),
        TunnelAction::Close(EndArg { port, machine: Some(machine), .. }) => {
            format!("closed the tunnel of port {port} of {machine}\n")
        }
        TunnelAction::Forget(EndArg { port, machine: Some(machine), .. }) => {
            format!("forgot the tunnel of port {port} of {machine}\n")
        }
        TunnelAction::Close(EndArg { port, reverse, .. }) => {
            format!("closed the {}tunnel of port {port}\n", if *reverse { "reverse " } else { "" })
        }
        TunnelAction::Forget(EndArg { port, reverse, .. }) => {
            format!("forgot the {}tunnel of port {port}\n", if *reverse { "reverse " } else { "" })
        }
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

fn machine_opened_text(port: u16, machine: &str, list: &TunnelList) -> String {
    let found =
        list.machine_tunnels.iter().find(|tunnel| tunnel.port == port && tunnel.machine.eq_ignore_ascii_case(machine));
    match found {
        Some(MachineTunnelInfo { state, local: Some(local), .. }) if state == "up" => {
            format!("port {port} of {machine} is tunnelled to localhost:{local}\n")
        }
        Some(tunnel) => {
            format!("port {port} of {machine} is not tunnelled: {}\n", tunnel.error.as_deref().unwrap_or("unknown"))
        }
        None => format!("port {port} of {machine} is not tunnelled\n"),
    }
}

pub(crate) fn text(list: &TunnelList) -> String {
    let there: String = list.machine_tunnels.iter().map(machine_line).collect();
    if list.tunnels.is_empty() {
        if !there.is_empty() {
            return there;
        }
        return if list.windows == 0 && list.takers == 0 { NO_WINDOW.into() } else { "no tunnels\n".into() };
    }
    let holders = list.tunnels.iter().map(|tunnel| tunnel.holder).collect::<BTreeSet<u64>>().len();
    let several = list.windows > 1 || holders > 1;
    list.tunnels.iter().map(|tunnel| line(tunnel, several)).chain(std::iter::once(there)).collect()
}

fn machine_line(tunnel: &MachineTunnelInfo) -> String {
    let at = format!("{} of {}", tunnel.port, tunnel.machine);
    match (tunnel.state.as_str(), tunnel.local) {
        ("up", Some(local)) => format!("{at} → localhost:{local}  up (this server)\n"),
        ("down", _) => format!("{at}  down: {} (this server)\n", tunnel.error.as_deref().unwrap_or("unknown")),
        (state, _) => format!("{at}  {state} (this server)\n"),
    }
}

fn line(tunnel: &TunnelInfo, several: bool) -> String {
    let mut text = format!("{}", tunnel.port);
    let arrow = arrow(tunnel.direction);
    match (tunnel.state.as_str(), tunnel.local) {
        ("up", Some(local)) => {
            let _ = write!(text, " {arrow} localhost:{local}  up");
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

pub(super) fn arrow(direction: Option<Direction>) -> &'static str {
    if direction == Some(Direction::Reverse) { "←" } else { "→" }
}

pub(super) fn hold_word(held: bool, direction: Option<Direction>) -> Option<&'static str> {
    match (direction, held) {
        (Some(Direction::Reverse), _) => Some("reverse hold"),
        (_, true) => Some("hold"),
        (_, false) => None,
    }
}

fn holder_suffix(tunnel: &TunnelInfo, several: bool) -> String {
    if let Some(word) = hold_word(tunnel.kind == "hold", tunnel.direction) {
        format!(" ({word} {})", tunnel.holder)
    } else if several {
        format!(" (window {})", tunnel.holder)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests;
