use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread;

use super::link::{self, Asked, Connect, Control, Envelope, Heard, Line};
use crate::control::Command;
use crate::error::Error;
use crate::panics;
use crate::remote::Killer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Followed {
    pub(crate) id: u64,
    pub(crate) event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    Line(String),
    Ended(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Wanted {
    pub(crate) command: Command,
    pub(crate) boot: String,
    pub(crate) once: bool,
}

pub(crate) struct Follow {
    id: u64,
    control: Arc<Control>,
    killer: Killer,
}

impl Follow {
    pub(crate) fn start<E>(id: u64, connector: Box<dyn Connect>, name: &str, wanted: Wanted, tx: &Sender<E>) -> Self
    where
        E: From<Followed> + Send + 'static,
    {
        let (control, killer) = (Arc::new(Control::default()), Killer::default());
        let running = Running {
            id,
            connector,
            name: name.into(),
            wanted,
            tx: tx.clone(),
            control: Arc::clone(&control),
            killer: killer.clone(),
        };
        thread::spawn(move || running.run());
        Self { id, control, killer }
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for Follow {
    fn drop(&mut self) {
        self.control.stop();
        self.killer.kill();
    }
}

struct Running<E> {
    id: u64,
    connector: Box<dyn Connect>,
    name: String,
    wanted: Wanted,
    tx: Sender<E>,
    control: Arc<Control>,
    killer: Killer,
}

impl<E: From<Followed>> Running<E> {
    fn run(self) {
        let followed = panics::job(|| {
            self.follow();
            Ok(())
        });
        if followed.is_err() {
            self.tell(Event::Ended(Error::Bug.to_string()));
        }
    }

    fn follow(&self) {
        let mut line = match self.connector.connect(&self.killer) {
            Ok(line) => line,
            Err(error) => return self.tell(Event::Ended(link::failure(&error, &self.name))),
        };
        if self.control.hold(&line.ends) == Asked::Nothing {
            self.hear(&mut line);
            self.control.release();
        }
        line.ends.cut();
    }

    fn hear(&self, line: &mut Line) {
        let envelope = Envelope { view: None, boot: Some(self.wanted.boot.clone()) };
        let mut heard = link::send_once(line, &envelope, self.wanted.command.clone());
        loop {
            match heard {
                Heard::Answer(text) => self.tell(Event::Line(text)),
                Heard::Ended(said) => return self.tell(Event::Ended(link::why(line, said))),
            }
            if self.wanted.once {
                return;
            }
            heard = link::listen(&mut line.reader);
        }
    }

    fn tell(&self, event: Event) {
        if !self.control.stopped() {
            let _ = self.tx.send(Followed { id: self.id, event }.into());
        }
    }
}

#[cfg(test)]
mod tests;
