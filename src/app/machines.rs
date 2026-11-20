use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::tunnels::Origin;
use crate::log;
use crate::machines::{self, form::Form};
use crate::protocol::{MachineOrder, Machines, Tunnel, TunnelOrder, TunnelState};
use crate::settings;
use crate::text;

mod shown;

pub(super) use shown::change_shown;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MachineItem {
    Here,
    To(u16),
    Menu(u16),
    Show(u16, bool),
    OnlyHere,
    ShowAll,
    Add,
    Manage,
    Tunnel(u16),
    TunnelDo(u16, TunnelVerb),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TunnelVerb {
    Browse,
    Close,
    Open,
    Forget,
}

impl TunnelVerb {
    fn label(self) -> &'static str {
        match self {
            Self::Browse => "open in browser",
            Self::Close => "close",
            Self::Open => "open",
            Self::Forget => "forget",
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct Window {
    client: u64,
    shown: Option<Machines>,
    menu: Vec<String>,
}

impl App {
    pub(crate) fn set_machines(&mut self, client: u64, machines: Option<Machines>) {
        self.opening.machines.client = client;
        self.opening.machines.shown = machines;
    }

    pub(super) fn window_in_use(&self) -> u64 {
        self.opening.machines.client
    }

    pub(crate) fn take_machine_orders(&mut self) -> Vec<(u64, MachineOrder)> {
        self.outbox.take_machine_orders()
    }

    pub(super) fn window_on(&self) -> Option<String> {
        self.opening.machines.shown.as_ref()?.here.clone()
    }

    fn menu_machines(&self) -> Vec<String> {
        let Some(shown) = &self.opening.machines.shown else { return Vec::new() };
        if shown.here.is_some() {
            return shown.known.clone();
        }
        machines::accepted(&self.config.machines).map(|machine| machine.name.clone()).collect()
    }

    pub(super) fn machine_label(&self) -> Option<String> {
        let shown = self.opening.machines.shown.as_ref()?;
        match &shown.here {
            Some(here) => Some(format!("on {here}")),
            None => (!self.config.machines.is_empty()).then(|| machines::HERE.into()),
        }
    }

    pub(super) fn open_machines(&mut self, at: Position) {
        let Some(here) = self.opening.machines.shown.as_ref().map(|shown| shown.here.clone()) else { return };
        let mut menu = self.menu_machines();
        if let Some(here) = here.as_ref().filter(|here| !menu.contains(here)) {
            menu.insert(0, here.clone());
        }
        let count = u16::try_from(menu.len()).unwrap_or(u16::MAX);
        self.opening.machines.menu = menu;
        let (machine, all): (fn(u16) -> MachineItem, &[MachineItem]) = match (&here, count) {
            (Some(_), _) => (MachineItem::To, &[]),
            (None, 0) => (MachineItem::Menu, &[MachineItem::Add, MachineItem::Manage]),
            (None, _) => (
                MachineItem::Menu,
                &[MachineItem::OnlyHere, MachineItem::ShowAll, MachineItem::Add, MachineItem::Manage],
            ),
        };
        let actions = std::iter::once(MachineItem::Here)
            .chain((0..count).map(machine))
            .chain(all.iter().copied())
            .chain(self.shown_tunnels().iter().map(|tunnel| MachineItem::Tunnel(tunnel.port)))
            .map(MenuAction::Machine)
            .collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn machine_menu_label(&self, action: MenuAction) -> Option<String> {
        let MenuAction::Machine(item) = action else { return None };
        let here = self.opening.machines.shown.as_ref().and_then(|shown| shown.here.as_deref());
        let mark = |name: Option<&str>| if name == here { "✓" } else { " " };
        Some(match item {
            MachineItem::Here => format!("{} {}", mark(None), machines::HERE),
            MachineItem::To(_) if here.is_none() => "open the window there".into(),
            MachineItem::To(index) => {
                let name = self.menu_machine(index).unwrap_or_default();
                format!("{} {name}", mark(Some(name)))
            }
            MachineItem::Menu(index) => self.machine_row_label(index),
            MachineItem::Show(_, true) => "show its work here ·".into(),
            MachineItem::Show(_, false) => "show its work here ✓".into(),
            MachineItem::OnlyHere => "only this computer".into(),
            MachineItem::ShowAll => "show all".into(),
            MachineItem::Add => "add machine…".into(),
            MachineItem::Manage => "manage machines…".into(),
            MachineItem::Tunnel(port) => self.tunnel_row(port),
            MachineItem::TunnelDo(_, verb) => verb.label().into(),
        })
    }

    fn machine_row_label(&self, index: u16) -> String {
        let longest = self.opening.machines.menu.iter().map(|name| text::cells(name)).max().unwrap_or_default();
        format!("  {} ▸", text::pad_cells(self.menu_machine(index).unwrap_or_default(), longest))
    }

    fn shown_now(&self, index: u16) -> Option<bool> {
        let name = self.menu_machine(index)?;
        machines::accepted(&self.config.machines).find(|machine| machine.name == name).map(|machine| machine.shown)
    }

    fn open_machine_menu(&mut self, index: u16, at: Position) {
        let Some(shown) = self.shown_now(index) else { return };
        let actions = [MachineItem::Show(index, !shown), MachineItem::To(index)].map(MenuAction::Machine).to_vec();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    pub(super) fn machine_disabled(&self, action: MenuAction) -> bool {
        let wanted = match action {
            MenuAction::Machine(MachineItem::OnlyHere) => false,
            MenuAction::Machine(MachineItem::ShowAll) => true,
            _ => return false,
        };
        machines::accepted(&self.config.machines).all(|machine| machine.shown == wanted)
    }

    fn tunnel_row(&self, port: u16) -> String {
        let tunnels = self.shown_tunnels();
        match tunnels.iter().find(|tunnel| tunnel.port == port) {
            Some(Tunnel { state: TunnelState::Up, local, .. }) => {
                format!("⇄ {port} → localhost:{}  up", local.unwrap_or(port))
            }
            Some(tunnel) => format!("⇄ {port}  {}", tunnel.state.name()),
            None => format!("⇄ {port}"),
        }
    }

    fn open_tunnel_menu(&mut self, port: u16, at: Position) {
        let Some(state) = self.shown_tunnels().into_iter().find(|tunnel| tunnel.port == port).map(|t| t.state) else {
            return;
        };
        let browse = self.shown_holder().is_some_and(|holder| holder.browse);
        let verbs: &[TunnelVerb] = match state {
            TunnelState::Up if browse => &[TunnelVerb::Browse, TunnelVerb::Close, TunnelVerb::Forget],
            TunnelState::Up | TunnelState::Opening => &[TunnelVerb::Close, TunnelVerb::Forget],
            TunnelState::Down(_) => &[TunnelVerb::Open, TunnelVerb::Close, TunnelVerb::Forget],
            TunnelState::Closed => &[TunnelVerb::Open, TunnelVerb::Forget],
        };
        let actions = verbs.iter().map(|verb| MenuAction::Machine(MachineItem::TunnelDo(port, *verb))).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn tunnel_action(&mut self, port: u16, verb: TunnelVerb) {
        match verb {
            TunnelVerb::Browse => self.browse_tunnel_here(port),
            TunnelVerb::Close => self.close_tunnel_of(port),
            TunnelVerb::Open => {
                let client = self.window_in_use();
                self.order_tunnel(client, TunnelOrder::Open(port), Some(Origin::Click));
            }
            TunnelVerb::Forget => self.forget_tunnel_of(port),
        }
    }

    fn menu_machine(&self, index: u16) -> Option<&str> {
        self.opening.machines.menu.get(usize::from(index)).map(String::as_str)
    }

    pub(super) fn machine_action(&mut self, item: MachineItem, at: Position) {
        match item {
            MachineItem::Here => self.switch_machine(None),
            MachineItem::To(index) => {
                let to = self.menu_machine(index).map(str::to_string);
                if to.is_some() {
                    self.switch_machine(to);
                }
            }
            MachineItem::Menu(index) => self.open_machine_menu(index, at),
            MachineItem::Show(index, shown) => {
                if let Some(name) = self.menu_machine(index).map(str::to_string) {
                    self.show_from_menu(Some(&name), shown);
                }
            }
            MachineItem::OnlyHere | MachineItem::ShowAll if self.machine_disabled(MenuAction::Machine(item)) => {}
            MachineItem::OnlyHere => self.show_from_menu(None, false),
            MachineItem::ShowAll => self.show_from_menu(None, true),
            MachineItem::Add => {
                self.screen.nav = None;
                self.overlay = Some(Overlay::Machine(Box::new(Form::from_button())));
            }
            MachineItem::Manage => {
                self.screen.nav = None;
                self.open_settings_on(&settings::Row::Here);
            }
            MachineItem::Tunnel(port) => self.open_tunnel_menu(port, at),
            MachineItem::TunnelDo(port, verb) => self.tunnel_action(port, verb),
        }
    }

    fn switch_machine(&mut self, to: Option<String>) {
        let here = self.opening.machines.shown.as_ref().and_then(|shown| shown.here.clone());
        if self.opening.machines.shown.is_none() || to == here {
            return;
        }
        log::info!("ui", "switch machine", client = self.opening.machines.client, here = to.is_none());
        self.outbox.order_machine(self.opening.machines.client, MachineOrder::Switch(to));
    }
}
