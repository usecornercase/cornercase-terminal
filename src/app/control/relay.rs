use serde_json::Value;

use super::{Asked, Handled, Pending, Stage, malformed};
use crate::app::App;
use crate::app::machine_links::RemoteItem;
use crate::app::machine_links::relay::{Sent, Then};
use crate::control::contract::Refusal;
use crate::control::{Command, Done, Start, Until};
use crate::machines::send::{Send, UNWAITED};
use crate::remote::hints;

pub(super) struct Relaying {
    sent: Sent,
    focus: bool,
}

impl App {
    pub(super) fn relay_request(&mut self, client: u64, machine: &str, send: Send) -> Handled {
        unwaited(&send)?;
        let focus = send.focus();
        let key = self.clients.requests.key();
        let target = self.send_to_machine(machine, send, Then::Client(key))?;
        let (client, done, stage) = (Some(client), Done::default(), Stage::Relay(Relaying { sent: target, focus }));
        self.clients.requests.pending.push(Pending { client, key, timeout: None, done, stage });
        Ok(None)
    }

    pub(super) fn start_with_request(&mut self, asked: Asked, start: Start) -> Handled {
        match start.machine.clone() {
            Some(machine) => self.relay_request(asked.client, &machine, Send::StartWith(start)),
            None => self.start_request(asked, start),
        }
    }

    pub(in crate::app) fn relay_answered(&mut self, key: u64, result: Result<Value, Refusal>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = match pending.stage {
            Stage::Relay(relaying) => self.relayed_reply(relaying, result),
            _ => result,
        };
        self.refuse_or_answer(pending.client, reply);
    }

    fn relayed_reply(&mut self, relaying: Relaying, result: Result<Value, Refusal>) -> Result<Value, Refusal> {
        let machine = relaying.sent.machine.clone();
        match result {
            Ok(value) => {
                let mut value = self.relayed_here(relaying, value);
                hints::errors_on_machine(&machine, &mut value);
                Ok(value)
            }
            Err(refusal) => Err(Refusal::new(refusal.code, hints::on_machine(&machine, &refusal.message))),
        }
    }

    fn relayed_here(&mut self, relaying: Relaying, mut value: Value) -> Value {
        let Relaying { sent, focus } = relaying;
        if focus {
            self.await_made(&sent, &value);
        }
        if let Value::Object(answer) = &mut value {
            answer.insert("machine".into(), Value::String(sent.machine));
        }
        value
    }

    pub(in crate::app) fn await_made(&mut self, sent: &Sent, value: &Value) {
        let id = |key: &str| value.pointer(key).and_then(Value::as_u64);
        let tab = id("/tab").or_else(|| id("/members/0/tab"));
        let made = tab.map(RemoteItem::Tab).or_else(|| id("/workspace").map(RemoteItem::Workspace));
        self.await_sent(sent, made);
    }
}

fn unwaited(send: &Send) -> Result<(), Refusal> {
    match send {
        Send::Start(start) | Send::StartWith(start)
            if start.wait || start.until != Until::Stops || start.timeout.is_some() =>
        {
            Err(malformed(UNWAITED))
        }
        _ => Ok(()),
    }
}

const LONE_MACHINE_BOOT: &str =
    "`machine_boot` goes with `machine`: it is the boot of that machine's ids; send `boot` for this server's";

pub(super) fn lone_machine_boot(command: &Command) -> Result<(), Refusal> {
    let (machine, boot) = match command {
        Command::Start(start) | Command::StartWith(start) => (&start.machine, &start.machine_boot),
        Command::NewWorkspace(new) => (&new.machine, &new.machine_boot),
        Command::FlowRun(run) => (&run.machine, &run.machine_boot),
        _ => return Ok(()),
    };
    match (machine, boot) {
        (None, Some(_)) => Err(malformed(LONE_MACHINE_BOOT)),
        _ => Ok(()),
    }
}

pub(super) fn viewless(command: &Command) -> Result<(), Refusal> {
    let (machine, focus) = match command {
        Command::Start(start) | Command::StartWith(start) => (&start.machine, start.focus),
        Command::NewWorkspace(new) => (&new.machine, new.focus),
        Command::FlowRun(run) => (&run.machine, run.focus),
        _ => return Ok(()),
    };
    match machine {
        Some(_) if focus => Err(malformed(
            "`focus` with `machine` takes no view: what a machine shows is the window's, shared by every client",
        )),
        _ => Ok(()),
    }
}
