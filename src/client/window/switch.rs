use std::thread;

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{Incoming, Next, Window};
use crate::client::{Ending, Link, Target};
use crate::error::{Error, Result};
use crate::protocol::{self, ClientMessage, MachineOrder, Machines};
use crate::remote;
use crate::remote::machines::{self, Machine};
use crate::ui::remote::{self as notice, Button, THIS_MACHINE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    Asking,
    Connecting,
    Failed(String),
}

pub(super) struct Switch {
    pub(super) to: Target,
    pub(super) step: Step,
    pub(super) hovered: Option<Button>,
    pub(super) left: Option<Next>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pressed {
    Connect,
    Back,
    Hover(Option<Button>),
}

pub(super) fn chosen(to: Option<String>, here: &Target, known: Vec<Machine>) -> Option<(Target, bool)> {
    let Some(destination) = to else {
        return (!matches!(here, Target::Local)).then_some((Target::Local, false));
    };
    if !machines::valid(&destination) || here.machine() == Some(destination.as_str()) {
        return None;
    }
    let known = known.into_iter().find(|machine| machine.destination == destination);
    let asks = known.is_none();
    let machine = known.unwrap_or(Machine { destination, command: None });
    Some((Target::Remote(machine.remote()), asks))
}

pub(super) fn pressed(step: &Step, ev: &Event, screen: Rect) -> Option<Pressed> {
    let shown = if *step == Step::Asking { [Button::Connect, Button::Back].as_slice() } else { &[Button::Back] };
    match ev {
        Event::Key(key) if key.kind == KeyEventKind::Release => None,
        Event::Key(key) if key.code == KeyCode::Esc => Some(Pressed::Back),
        Event::Key(key) if key.code == KeyCode::Enter && *step == Step::Asking => Some(Pressed::Connect),
        Event::Mouse(mouse) => {
            let on = notice::hit(screen, shown, Position::new(mouse.column, mouse.row));
            match (on, mouse.kind) {
                (Some(Button::Connect), MouseEventKind::Down(MouseButton::Left)) => Some(Pressed::Connect),
                (Some(_), MouseEventKind::Down(MouseButton::Left)) => Some(Pressed::Back),
                _ => Some(Pressed::Hover(on)),
            }
        }
        _ => None,
    }
}

fn reach(to: &Target) -> Result<Link> {
    if matches!(to, Target::Local) && std::env::var_os(protocol::NESTED_ENV).is_some() {
        return Err(Error::Nested);
    }
    to.connect(true)
}

pub(super) fn failure(error: &Error) -> String {
    if remote::fatal(error) { error.to_string() } else { remote::reason(error) }
}

pub(super) fn place(target: &Target) -> &str {
    target.machine().unwrap_or(THIS_MACHINE)
}

impl Window<'_> {
    pub(super) fn machines(&self) -> Machines {
        let here = self.target.machine().map(str::to_string);
        Machines { here, known: machines::read(&machines::file()).into_iter().map(|m| m.destination).collect() }
    }

    pub(super) fn remember(&self) {
        if let Target::Remote(remote) = &self.target {
            machines::remember(&machines::file(), Machine::of(remote));
        }
    }

    pub(super) fn order(&mut self, order: MachineOrder) -> Result<()> {
        match order {
            MachineOrder::Forget(destination) => {
                machines::forget(&machines::file(), &destination);
                self.send_out(ClientMessage::Machines(self.machines()));
                Ok(())
            }
            MachineOrder::Switch(to) => self.begin(to),
        }
    }

    fn begin(&mut self, to: Option<String>) -> Result<()> {
        if self.switch.is_some() || self.lost.is_some() {
            return Ok(());
        }
        let Some((to, asks)) = chosen(to, &self.target, machines::read(&machines::file())) else { return Ok(()) };
        let step = if asks { Step::Asking } else { Step::Connecting };
        self.switch = Some(Switch { to, step, hovered: None, left: None });
        if !asks {
            self.connect();
        }
        self.show_notice()
    }

    fn connect(&mut self) {
        let Some(switch) = &mut self.switch else { return };
        switch.step = Step::Connecting;
        self.switches += 1;
        let (to, tx, id) = (switch.to.clone(), self.tx.clone(), self.switches);
        thread::spawn(move || {
            let _ = tx.send(Incoming::Switched(id, reach(&to)));
        });
    }

    pub(super) fn switched(&mut self, id: u64, linked: Result<Link>) -> Result<()> {
        if id != self.switches {
            return Ok(());
        }
        let Some(switch) = &mut self.switch else { return Ok(()) };
        match linked {
            Ok(link) => {
                let Some(switch) = self.switch.take() else { return Ok(()) };
                self.out = None;
                self.guard = None;
                self.target = switch.to;
                self.retarget();
                self.link(link)
            }
            Err(error) => {
                switch.step = Step::Failed(failure(&error));
                self.show_notice()
            }
        }
    }

    pub(super) fn switch_input(&mut self, ev: &Event) -> Result<Option<Ending>> {
        let screen = self.screen()?;
        let Some(switch) = &mut self.switch else { return Ok(None) };
        if matches!(ev, Event::Resize(..)) {
            self.wipe()?;
            self.show_notice()?;
            return Ok(None);
        }
        match pressed(&switch.step, ev, screen) {
            Some(Pressed::Connect) => {
                self.connect();
                self.show_notice()?;
            }
            Some(Pressed::Back) => return self.back(),
            Some(Pressed::Hover(on)) if on != switch.hovered => {
                switch.hovered = on;
                self.show_notice()?;
            }
            Some(Pressed::Hover(_)) | None => {}
        }
        Ok(None)
    }

    fn back(&mut self) -> Result<Option<Ending>> {
        self.switches += 1;
        let Some(switch) = self.switch.take() else { return Ok(None) };
        match switch.left {
            Some(Next::End(ending)) => return Ok(Some(ending)),
            Some(Next::Reconnect) => self.lose()?,
            _ => {
                let size = self.terminal.size()?;
                self.send_out(ClientMessage::Event(Event::Resize(size.width, size.height)));
            }
        }
        Ok(None)
    }

    pub(super) fn while_switching(&mut self, next: Next) -> Option<Next> {
        let switch = self.switch.as_mut()?;
        match next {
            Next::End(_) | Next::Reconnect => switch.left = Some(next),
            Next::Show(_) | Next::Machine(_) | Next::Open(_) | Next::Browse(_) | Next::Nothing => {}
            Next::Sound(_) => return Some(next),
        }
        None
    }
}

impl Switch {
    pub(super) fn shown<'a>(&'a self, from: &'a Target) -> notice::Switch<'a> {
        let step = match &self.step {
            Step::Asking => notice::Step::Asking,
            Step::Connecting => notice::Step::Connecting,
            Step::Failed(error) => notice::Step::Failed(error),
        };
        notice::Switch { from: place(from), to: place(&self.to), step, hovered: self.hovered }
    }
}

#[cfg(test)]
mod tests;
