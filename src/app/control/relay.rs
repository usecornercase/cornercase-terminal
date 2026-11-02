use serde_json::Value;

use super::{Asked, Handled, Pending, Stage, malformed};
use crate::app::App;
use crate::app::machines::RemoteItem;
use crate::app::machines::relay::{Sent, Then};
use crate::control::contract::Refusal;
use crate::control::{Done, Start, Until};
use crate::log;
use crate::machines::send::{Send, UNWAITED};

pub(super) struct Relaying {
    sent: Sent,
    focus: bool,
}

impl App {
    pub(super) fn relay_request(&mut self, client: u64, machine: &str, send: Send) -> Handled {
        unwaited(&send)?;
        let focus = focused(&send);
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
            Stage::Relay(relaying) => result.map(|value| self.relayed_here(relaying, value)),
            _ => result,
        };
        self.refuse_or_answer(pending.client, reply);
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
        let id = |key: &str| value.get(key).and_then(Value::as_u64);
        let made = id("tab").map(RemoteItem::Tab).or_else(|| id("workspace").map(RemoteItem::Workspace));
        match (made, sent.boot.clone()) {
            (Some(made), Some(boot)) => self.await_remote(sent.link, boot, made),
            _ => log::info!("machines", "nothing to select", name = sent.machine),
        }
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

fn focused(send: &Send) -> bool {
    match send {
        Send::Start(start) | Send::StartWith(start) => start.focus,
        Send::NewWorkspace(new) => new.focus,
        Send::FlowRun(run) => run.focus,
        Send::NewTab(tab) => tab.focus,
        Send::Clone(clone) => clone.focus,
        Send::AccountList | Send::IssueSources(_) | Send::FlowList(_) => false,
    }
}
