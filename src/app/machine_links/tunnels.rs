use std::ffi::OsString;
use std::fmt;
use std::net::Ipv6Addr;
use std::time::Instant;

use super::MachineEvent;
use super::panels::RemoteAim;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::toast::{Toast, TunnelButton};
use crate::app::tunnels::Origin;
use crate::control::{Command, HoldEnd, MachineTunnelInfo, TunnelHold};
use crate::error::Error;
use crate::host::open::Browse;
use crate::log::{self, Job, Level};
use crate::machines::follow::{Event, Follow, Followed};
use crate::machines::heard;
use crate::ports::Port;
use crate::ports::dialog::There;
use crate::protocol::TunnelState;
use crate::remote::tunnel::{self, Forward, Running, UP_WAIT, Watched};
use crate::remote::{self, SSH};
use crate::ui;

pub(in crate::app) const MAX_THERE: usize = 32;
const NOT_HERE: &str = "a tunnel to a machine listens on the computer of cornercase's server, and this window is on \
    another computer";

pub(crate) struct Spawned {
    local: u16,
    running: Running,
}

impl fmt::Debug for Spawned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Spawned").field("local", &self.local).field("pid", &self.running.pid()).finish()
    }
}

#[derive(Debug)]
pub(crate) enum TunnelHeard {
    Spawned { id: u64, result: Result<Spawned, String> },
    Watched { id: u64, watched: Watched },
}

pub(in crate::app) struct ThereTunnels {
    by: Vec<ThereTunnel>,
    next: u64,
    ssh: OsString,
}

impl Default for ThereTunnels {
    fn default() -> Self {
        Self { by: Vec::new(), next: 1, ssh: OsString::from(SSH) }
    }
}

impl ThereTunnels {
    fn find(&self, link: u64, port: u16) -> Option<&ThereTunnel> {
        self.by.iter().find(|tunnel| tunnel.link == link && tunnel.port == port)
    }

    fn by_id(&mut self, id: u64) -> Option<&mut ThereTunnel> {
        self.by.iter_mut().find(|tunnel| tunnel.id == id)
    }

    fn running(&self) -> usize {
        self.by.iter().filter(|tunnel| tunnel.running.is_some() || tunnel.state == TunnelState::Opening).count()
    }

    #[cfg(test)]
    pub(in crate::app) fn set_ssh(&mut self, ssh: OsString) {
        self.ssh = ssh;
    }
}

struct ThereTunnel {
    id: u64,
    link: u64,
    machine: String,
    destination: String,
    port: u16,
    local: Option<u16>,
    state: TunnelState,
    running: Option<Running>,
    early: Option<Watched>,
    hold: Option<(Follow, String)>,
    hold_at: Option<Instant>,
    holds_failed: u32,
    asked: Option<Origin>,
    was_up: bool,
    failed: u32,
    down_at: Option<Instant>,
}

impl ThereTunnel {
    fn new(id: u64, (link, machine, destination): (u64, String, String), port: u16, origin: Origin) -> Self {
        Self {
            id,
            link,
            machine,
            destination,
            port,
            local: None,
            state: TunnelState::Opening,
            running: None,
            early: None,
            hold: None,
            hold_at: None,
            holds_failed: 0,
            asked: Some(origin),
            was_up: false,
            failed: 0,
            down_at: None,
        }
    }

    fn info(&self) -> MachineTunnelInfo {
        let error = match &self.state {
            TunnelState::Down(reason) => Some(reason.clone()),
            _ => None,
        };
        let registered = self.hold.is_some().then_some(true);
        MachineTunnelInfo {
            machine: self.machine.clone(),
            port: self.port,
            local: self.local,
            state: self.state.name().into(),
            error,
            registered,
        }
    }

    fn port_tunnel(&self) -> ui::PortTunnel {
        match (&self.state, self.local) {
            (TunnelState::Up, Some(local)) => ui::PortTunnel::Up(local),
            (TunnelState::Opening | TunnelState::Up, _) => ui::PortTunnel::Opening,
            (TunnelState::Down(_) | TunnelState::Closed, _) => ui::PortTunnel::Offered,
        }
    }
}

impl App {
    pub(in crate::app) fn tunnels_here(&self) -> bool {
        self.window_on().is_none()
    }

    pub(in crate::app) fn there_tunnel(&self, there: &There, port: &Port) -> Option<ui::PortTunnel> {
        if !self.tunnels_here() || port.browse().ip.is_some_and(|ip| ip != Ipv6Addr::LOCALHOST) {
            return None;
        }
        let found = self.machines.tunnels.find(there.link, port.number);
        Some(found.map_or(ui::PortTunnel::Offered, ThereTunnel::port_tunnel))
    }

    pub(in crate::app) fn tunnel_there(&mut self, there: &There, port: u16) {
        if let Err(why) = self.open_there_tunnel(there.link, port, Origin::Click) {
            self.messages.toast = Some(Toast::new(format!("could not tunnel port {port}: {why}"), ui::ToastIcon::Bug));
        }
    }

    pub(in crate::app) fn open_there_port(&mut self, there: &There, port: &Port) {
        if let Some(ui::PortTunnel::Up(local)) = self.there_tunnel(there, port) {
            self.browse(Browse { ip: None, port: local });
        }
    }

    pub(in crate::app) fn close_there_tunnel_of(&mut self, machine: &str, port: u16) {
        if let Some(link) = self.there_link(machine)
            && self.close_there_tunnel(link, port)
        {
            let message = format!("closed the tunnel of port {port} of {machine}");
            self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
        }
    }

    pub(in crate::app) fn there_link(&self, machine: &str) -> Option<u64> {
        let held = self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(machine))?;
        Some(held.link.id())
    }

    pub(in crate::app) fn open_there_tunnel(&mut self, link: u64, port: u16, origin: Origin) -> Result<(), String> {
        if port == 0 {
            return Err("a port is 1 to 65535".into());
        }
        if origin == Origin::Click && !self.tunnels_here() {
            return Err(NOT_HERE.into());
        }
        let held = self.held_by(link).ok_or("that machine is no longer listed")?;
        let (machine, destination) = (held.name.clone(), held.destination.clone());
        let tunnels = &mut self.machines.tunnels;
        if let Some(tunnel) = tunnels.by.iter_mut().find(|tunnel| tunnel.link == link && tunnel.port == port) {
            if matches!(tunnel.state, TunnelState::Up | TunnelState::Opening) {
                tunnel.asked.get_or_insert(origin);
                return Ok(());
            }
            tunnel.asked = Some(origin);
            let id = tunnel.id;
            self.spawn_there(id);
            return Ok(());
        }
        if tunnels.running() >= MAX_THERE {
            return Err(format!("at most {MAX_THERE} tunnels to machines are held at once"));
        }
        let id = tunnels.next;
        tunnels.next += 1;
        tunnels.by.push(ThereTunnel::new(id, (link, machine, destination), port, origin));
        self.spawn_there(id);
        Ok(())
    }

    fn spawn_there(&mut self, id: u64) {
        let ssh = self.machines.tunnels.ssh.clone();
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        tunnel.state = TunnelState::Opening;
        tunnel.early = None;
        let (port, before, destination) = (tunnel.port, tunnel.local, tunnel.destination.clone());
        let job = Job::new(Level::Info, "machines", "tunnel").with("name", tunnel.machine.clone()).with("port", port);
        let tx = self.tx.clone();
        self.spawn_job(
            job,
            move || {
                let local = tunnel::local_port(port, before).map_err(|error| Error::Control(error.to_string()))?;
                let report = move |watched| {
                    let heard = TunnelHeard::Watched { id, watched };
                    let _ = tx.send(AppEvent::Machine(MachineEvent::Tunnel(heard)));
                };
                let running = tunnel::spawn_within(&ssh, &destination, Forward { port, local }, UP_WAIT, report)?;
                Ok(Spawned { local, running })
            },
            move |result, _| {
                let heard = TunnelHeard::Spawned { id, result: result.map_err(|error| error.to_string()) };
                Some(AppEvent::Machine(MachineEvent::Tunnel(heard)))
            },
        );
    }

    pub(in crate::app) fn there_tunnel_heard(&mut self, heard: TunnelHeard) {
        match heard {
            TunnelHeard::Spawned { id, result } => self.there_spawned(id, result),
            TunnelHeard::Watched { id, watched } => self.there_watched(id, watched),
        }
    }

    fn there_spawned(&mut self, id: u64, result: Result<Spawned, String>) {
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        match result {
            Ok(spawned) => {
                tunnel.local = Some(spawned.local);
                tunnel.running = Some(spawned.running);
                if let Some(watched) = tunnel.early.take() {
                    self.there_watched(id, watched);
                }
            }
            Err(reason) => self.there_ended(id, &reason),
        }
    }

    fn there_watched(&mut self, id: u64, watched: Watched) {
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        if tunnel.running.is_none() && tunnel.state == TunnelState::Opening {
            tunnel.early = Some(watched);
            return;
        }
        match watched {
            Watched::Up => self.there_up(id),
            Watched::Ended(reason) => self.there_ended(id, &reason),
        }
    }

    fn there_up(&mut self, id: u64) {
        let browse = self.can_browse();
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        tunnel.state = TunnelState::Up;
        tunnel.was_up = true;
        tunnel.failed = 0;
        let (port, local, machine) = (tunnel.port, tunnel.local.unwrap_or(tunnel.port), tunnel.machine.clone());
        log::info!("machines", "tunnel up", name = machine, port = port, local = local);
        let toast = tunnel.asked.take().map(|origin| up_toast(&machine, (port, local), origin, browse));
        if toast.is_some() {
            self.messages.toast = toast;
        }
        self.clients.events.snapshots.mark_changed();
        self.register_there(id);
    }

    fn there_ended(&mut self, id: u64, reason: &str) {
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        let reason = tunnel::reason(reason);
        let reason = crate::machines::hide_destination(&reason, &tunnel.machine, &tunnel.destination);
        tunnel.running = None;
        tunnel.hold = None;
        tunnel.state = TunnelState::Down(reason.clone());
        tunnel.down_at = Some(Instant::now());
        tunnel.failed += 1;
        log::warning!("machines", "tunnel down", name = tunnel.machine, port = tunnel.port, bytes = reason.len());
        if tunnel.asked.take().is_some() || !tunnel.was_up {
            let message = format!("could not tunnel port {} of {}: {reason}", tunnel.port, tunnel.machine);
            self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
        }
        self.clients.events.snapshots.mark_changed();
    }

    pub(in crate::app) fn close_there_tunnel(&mut self, link: u64, port: u16) -> bool {
        let tunnels = &mut self.machines.tunnels;
        let Some(at) = tunnels.by.iter().position(|tunnel| tunnel.link == link && tunnel.port == port) else {
            return false;
        };
        let tunnel = tunnels.by.remove(at);
        log::info!("machines", "tunnel closed", name = tunnel.machine, port = port);
        if let Some(running) = tunnel.running {
            running.stop();
        }
        self.clients.events.snapshots.mark_changed();
        true
    }

    fn register_there(&mut self, id: u64) {
        let Some(tunnel) = self.machines.tunnels.by.iter().find(|tunnel| tunnel.id == id) else { return };
        let (link, port, local) = (tunnel.link, tunnel.port, tunnel.local.unwrap_or(tunnel.port));
        let Some(boot) = self.held_by(link).filter(|_| self.linked(link)).and_then(|held| held.seen.as_deref()) else {
            return;
        };
        let aim = RemoteAim { link, boot: boot.boot.clone(), workspace: 0 };
        let command = Command::TunnelHold(TunnelHold { port, local, direction: None });
        let follow = self.follow_machine(&aim, command, false);
        if let Some(tunnel) = self.machines.tunnels.by_id(id) {
            tunnel.hold_at = Some(Instant::now());
            tunnel.hold = follow.map(|follow| (follow, aim.boot));
        }
    }

    pub(in crate::app) fn there_hold_heard(&mut self, followed: &Followed) -> bool {
        let holds = |tunnel: &ThereTunnel| tunnel.hold.as_ref().is_some_and(|(follow, _)| follow.id() == followed.id);
        let Some(tunnel) = self.machines.tunnels.by.iter_mut().find(|tunnel| holds(tunnel)) else { return false };
        match &followed.event {
            Event::Line(text) => {
                if heard::held(text) {
                    tunnel.holds_failed = 0;
                }
                let end = heard::hold_ended(text).filter(|end| !end.server_stopped());
                let (link, port, machine) = (tunnel.link, tunnel.port, tunnel.machine.clone());
                if let Some(end) = end {
                    self.there_ended_there(link, (port, &machine), end);
                }
            }
            Event::Ended(_) => {
                tunnel.hold = None;
                tunnel.holds_failed += 1;
                self.clients.events.snapshots.mark_changed();
            }
        }
        true
    }

    fn there_ended_there(&mut self, link: u64, (port, machine): (u16, &str), end: HoldEnd) {
        self.close_there_tunnel(link, port);
        let verb = if end == HoldEnd::Forgotten { "forgot" } else { "closed" };
        let message = format!("{machine} {verb} the tunnel of its port {port}");
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Check));
    }

    pub(in crate::app) fn sync_there_tunnels(&mut self, now: Instant) {
        let listed: Vec<(u64, String)> =
            self.machines.held().iter().map(|held| (held.link.id(), held.destination.clone())).collect();
        let gone: Vec<(u64, u16)> = self
            .machines
            .tunnels
            .by
            .iter()
            .filter(|tunnel| !listed.iter().any(|(link, to)| *link == tunnel.link && *to == tunnel.destination))
            .map(|tunnel| (tunnel.link, tunnel.port))
            .collect();
        for (link, port) in gone {
            self.close_there_tunnel(link, port);
        }
        let ids: Vec<u64> = self.machines.tunnels.by.iter().map(|tunnel| tunnel.id).collect();
        for id in ids {
            self.keep_there(id, now);
        }
    }

    fn keep_there(&mut self, id: u64, now: Instant) {
        let Some(tunnel) = self.machines.tunnels.by.iter().find(|tunnel| tunnel.id == id) else { return };
        let link = tunnel.link;
        let boot = self.held_by(link).and_then(|held| held.seen.as_deref()).map(|seen| seen.boot.clone());
        let linked = self.linked(link);
        let Some(tunnel) = self.machines.tunnels.by_id(id) else { return };
        match &tunnel.state {
            TunnelState::Up => {
                if tunnel.hold.as_ref().is_some_and(|(_, held)| Some(held) != boot.as_ref()) {
                    tunnel.hold = None;
                }
                let due = tunnel.hold_at.is_none_or(|at| now >= at + remote::backoff(tunnel.holds_failed));
                if tunnel.hold.is_none() && linked && due {
                    self.register_there(id);
                }
            }
            TunnelState::Down(_) if tunnel.was_up && linked => {
                let due = tunnel.down_at.is_none_or(|at| now >= at + remote::backoff(tunnel.failed));
                if due {
                    self.spawn_there(id);
                }
            }
            _ => {}
        }
    }

    pub(in crate::app) fn there_tunnel_infos(&self) -> Vec<MachineTunnelInfo> {
        let mut infos: Vec<MachineTunnelInfo> = self.machines.tunnels.by.iter().map(ThereTunnel::info).collect();
        infos.sort_by(|a, b| (a.machine.as_str(), a.port).cmp(&(b.machine.as_str(), b.port)));
        infos
    }

    pub(in crate::app) fn there_tunnel_info(&self, link: u64, port: u16) -> Option<MachineTunnelInfo> {
        self.machines.tunnels.find(link, port).map(ThereTunnel::info)
    }
}

fn up_toast(machine: &str, (port, local): (u16, u16), origin: Origin, browse: bool) -> Toast {
    let mut toast = match origin {
        Origin::Click => {
            let taken = if local == port { String::new() } else { format!(" ({port} is taken here)") };
            let message = format!("port {port} of {machine} is tunnelled to localhost:{local}{taken}");
            let mut toast = Toast::new(message, ui::ToastIcon::Check);
            toast.lingers = true;
            toast
        }
        Origin::Request => Toast::new(
            format!("a program tunnelled port {port} of {machine} to localhost:{local}"),
            ui::ToastIcon::Check,
        ),
    };
    toast.tunnel = browse.then_some(TunnelButton::Local(local));
    toast
}
