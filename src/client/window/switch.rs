use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::{INCOMPATIBLE, Incoming, Next, Received, Window, openers_for, spawn_reader, spawn_writer};
use crate::client::{Ending, Guard, Link, Target};
use crate::error::{Error, Result};
use crate::host::open::Openers;
use crate::protocol::{self, ClientMessage, MachineOrder, Machines, ServerMessage};
use crate::remote;
use crate::remote::machines::{self, Machine};
use crate::ui::remote::{self as notice, Button, THIS_MACHINE};

const ANSWER_WAIT: Duration = Duration::from_secs(30);
const NO_ANSWER: &str = "its cornercase server did not answer within 30 s";
const CLOSED: &str = "the connection closed before its cornercase server answered";

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
    pending: Option<Pending>,
}

struct Pending {
    id: u64,
    out: Sender<ClientMessage>,
    guard: Guard,
    openers: Openers,
    size: (u16, u16),
    until: Instant,
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
    let machine = known
        .into_iter()
        .find(|machine| machine.destination == destination)
        .unwrap_or_else(|| Machine::never_reached(destination, None));
    let asks = machine.unreached;
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

pub(super) fn answer(received: Received) -> std::result::Result<ServerMessage, Error> {
    match received {
        Received::Message(ServerMessage::Rejected(reason)) => Err(Error::Rejected(reason)),
        Received::Message(first) => Ok(first),
        Received::Invalid => Err(Error::Rejected(INCOMPATIBLE.into())),
        Received::Ended => Err(Error::Control(CLOSED.into())),
    }
}

pub(super) fn kept(left: Option<Next>, next: Next) -> Option<Next> {
    left.or(Some(next))
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
    pub(super) fn machines_at(here: &Target) -> Machines {
        let known = machines::read(&machines::file()).into_iter().map(|m| m.destination).collect();
        Machines { here: here.machine().map(str::to_string), known }
    }

    pub(super) fn remember(&self) {
        if let Target::Remote(remote) = &self.target {
            let _ = machines::remember(&machines::file(), Machine::of(remote));
        }
    }

    pub(super) fn order(&mut self, order: MachineOrder) -> Result<()> {
        match order {
            MachineOrder::Forget(destination) => {
                let _ = machines::forget(&machines::file(), &destination);
                self.send_out(ClientMessage::Machines(Self::machines_at(&self.target)));
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
        self.switch = Some(Switch { to, step, hovered: None, left: None, pending: None });
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
        match linked {
            Ok(link) => self.offer(link),
            Err(error) => self.fail(&error),
        }
    }

    fn offer(&mut self, link: Link) -> Result<()> {
        let Some(to) = self.switch.as_ref().map(|switch| switch.to.clone()) else { return Ok(()) };
        let openers = openers_for(&to);
        let hello = self.hello(&to, &openers)?;
        let size = (hello.cols, hello.rows);
        let id = self.next_id();
        let Link { reader, writer, guard } = link;
        let out = spawn_writer(writer);
        let _ = out.send(ClientMessage::Hello(Box::new(hello)));
        spawn_reader(id, reader, self.tx.clone());
        let until = Instant::now() + ANSWER_WAIT;
        if let Some(switch) = &mut self.switch {
            switch.pending = Some(Pending { id, out, guard, openers, size, until });
        }
        Ok(())
    }

    pub(super) fn pending_id(&self) -> Option<u64> {
        self.switch.as_ref()?.pending.as_ref().map(|pending| pending.id)
    }

    pub(super) fn answered(&mut self, received: Received) -> Result<Option<Ending>> {
        match answer(received) {
            Ok(first) => self.commit(first),
            Err(error) => {
                let said = self.switch.as_ref().and_then(|s| s.pending.as_ref()?.guard.stderr.as_ref()?.last());
                let error = match (error, said) {
                    (Error::Control(_), Some(said)) => Error::Control(said),
                    (error, _) => error,
                };
                self.fail(&error)?;
                Ok(None)
            }
        }
    }

    fn commit(&mut self, first: ServerMessage) -> Result<Option<Ending>> {
        let Some(switch) = self.switch.take() else { return Ok(None) };
        let Some(pending) = switch.pending else { return Ok(None) };
        self.out = Some(pending.out);
        self.guard = Some(pending.guard);
        self.generation = pending.id;
        self.target = switch.to;
        self.openers = pending.openers;
        self.sync_plugins();
        self.remember();
        self.send_out(ClientMessage::Machines(Self::machines_at(&self.target)));
        self.lost = None;
        self.retries.linked();
        let size = self.terminal.size()?;
        if (size.width, size.height) != pending.size {
            self.send_out(ClientMessage::Event(Event::Resize(size.width, size.height)));
        }
        self.handle_server(Received::Message(first))
    }

    fn fail(&mut self, error: &Error) -> Result<()> {
        let Some(switch) = &mut self.switch else { return Ok(()) };
        switch.pending = None;
        switch.step = Step::Failed(failure(error));
        self.show_notice()
    }

    pub(super) fn switch_deadline(&self) -> Option<Instant> {
        self.switch.as_ref()?.pending.as_ref().map(|pending| pending.until)
    }

    pub(super) fn expire_switch(&mut self, now: Instant) -> Result<()> {
        if self.switch_deadline().is_some_and(|until| until <= now) {
            self.fail(&Error::Control(NO_ANSWER.into()))?;
        }
        Ok(())
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
            Next::End(_) | Next::Reconnect => switch.left = kept(switch.left.take(), next),
            Next::Show(_) | Next::Machine(_) | Next::Nothing => {}
            Next::Sound(_) | Next::Open(_) | Next::Browse(_) => return Some(next),
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
