use ratatui::layout::Position;

use super::App;
use super::menus::MenuAction;
use super::overlay::Overlay;
use super::tunnels::Origin;
use crate::editor::Editor;
use crate::log;
use crate::protocol::{MachineOrder, Machines, Tunnel, TunnelOrder, TunnelState};
use crate::remote::machines::{NOT_A_DESTINATION, valid};
use crate::ui::{self, remote::THIS_MACHINE};

pub(super) const CONNECT_SUBMIT: &str = "connect";

const HINT: &str = "anything ssh takes: a host of ~/.ssh/config or user@host";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MachineItem {
    Here,
    To(u16),
    Add,
    Forgetting,
    Forget(u16),
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

    pub(super) fn machine_label(&self) -> Option<String> {
        let shown = self.opening.machines.shown.as_ref()?;
        let switches = shown.here.is_some() || !shown.known.is_empty();
        switches.then(|| shown.here.clone().unwrap_or_else(|| THIS_MACHINE.into()))
    }

    pub(super) fn open_machines(&mut self, at: Position) {
        let Some(shown) = &self.opening.machines.shown else { return };
        let mut menu = shown.known.clone();
        if let Some(here) = shown.here.as_ref().filter(|here| !menu.contains(here)) {
            menu.insert(0, here.clone());
        }
        let count = u16::try_from(menu.len()).unwrap_or(u16::MAX);
        let forget = (!shown.known.is_empty()).then_some(MachineItem::Forgetting);
        self.opening.machines.menu = menu;
        let actions = std::iter::once(MachineItem::Here)
            .chain((0..count).map(MachineItem::To))
            .chain([MachineItem::Add])
            .chain(forget)
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
            MachineItem::Here => format!("{} {THIS_MACHINE}", mark(None)),
            MachineItem::To(index) => {
                let name = self.menu_machine(index).unwrap_or_default();
                format!("{} {name}", mark(Some(name)))
            }
            MachineItem::Add => "add machine…".into(),
            MachineItem::Forgetting => "forget…".into(),
            MachineItem::Forget(index) => format!("forget {}", self.menu_machine(index).unwrap_or_default()),
            MachineItem::Tunnel(port) => self.tunnel_row(port),
            MachineItem::TunnelDo(_, verb) => verb.label().into(),
        })
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
            MachineItem::Add => {
                self.screen.nav = None;
                self.overlay = Some(Overlay::NewMachine { input: Editor::default(), error: None });
            }
            MachineItem::Forgetting => self.open_forget_menu(at),
            MachineItem::Tunnel(port) => self.open_tunnel_menu(port, at),
            MachineItem::TunnelDo(port, verb) => self.tunnel_action(port, verb),
            MachineItem::Forget(index) => {
                if let Some(destination) = self.menu_machine(index).map(str::to_string) {
                    log::info!("ui", "forget machine", client = self.opening.machines.client);
                    self.outbox.order_machine(self.opening.machines.client, MachineOrder::Forget(destination));
                }
            }
        }
    }

    fn open_forget_menu(&mut self, at: Position) {
        let Some(shown) = &self.opening.machines.shown else { return };
        self.opening.machines.menu.clone_from(&shown.known);
        let count = u16::try_from(self.opening.machines.menu.len()).unwrap_or(u16::MAX);
        let actions = (0..count).map(|index| MenuAction::Machine(MachineItem::Forget(index))).collect();
        self.overlay = Some(Overlay::Menu { at, actions });
    }

    fn switch_machine(&mut self, to: Option<String>) {
        let here = self.opening.machines.shown.as_ref().and_then(|shown| shown.here.clone());
        if self.opening.machines.shown.is_none() || to == here {
            return;
        }
        log::info!("ui", "switch machine", client = self.opening.machines.client, here = to.is_none());
        self.outbox.order_machine(self.opening.machines.client, MachineOrder::Switch(to));
    }

    pub(super) fn new_machine_view(input: &Editor, error: Option<&str>) -> ui::Overlay {
        ui::Overlay::Form(ui::Form {
            title: "Add machine",
            label: "ssh destination",
            value: input.clone(),
            hint: HINT.into(),
            toggle: None,
            note: error.map(|error| ui::Note::Error(error.into())),
            submit: CONNECT_SUBMIT,
        })
    }

    pub(super) fn submit_new_machine(&mut self, input: Editor) -> Option<Overlay> {
        let destination = input.text().trim().to_string();
        if destination.is_empty() {
            return Some(Overlay::NewMachine { input, error: None });
        }
        if !valid(&destination) {
            return Some(Overlay::NewMachine { input, error: Some(NOT_A_DESTINATION.into()) });
        }
        self.switch_machine(Some(destination));
        None
    }
}
