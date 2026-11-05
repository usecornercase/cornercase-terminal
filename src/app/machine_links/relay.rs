use std::time::Duration;

use serde_json::Value;

use super::MachineEvent;
use super::checks::Checking;
use crate::app::App;
use crate::app::control::machines::unknown;
use crate::app::event::AppEvent;
use crate::app::machine_links::RemoteItem;
use crate::control::Command;
use crate::control::contract::{Code, Refusal};
use crate::log::{self, Job, Level};
use crate::machines::{self, State, act, ask, send};

type Hear = fn(&str) -> Result<Value, Refusal>;

pub(in crate::app) struct Asking {
    pub(in crate::app) machine: String,
    pub(in crate::app) destination: String,
    pub(in crate::app) boot: Option<String>,
    pub(in crate::app) timeout: Duration,
}

impl Asking {
    pub(in crate::app) fn hide(&self, why: &str) -> String {
        machines::hide_destination(why, &self.machine, &self.destination)
    }

    pub(in crate::app) fn heard(&self, result: Result<String, String>, job: &mut Job) -> Result<Value, Refusal> {
        self.heard_as(result, job, act::heard)
    }

    fn heard_as(&self, result: Result<String, String>, job: &mut Job, hear: Hear) -> Result<Value, Refusal> {
        let (heard, failed) = self.read(result, hear);
        if let Some(failed) = failed {
            job.fail(failed);
        }
        heard
    }

    pub(in crate::app) fn read(
        &self,
        result: Result<String, String>,
        hear: Hear,
    ) -> (Result<Value, Refusal>, Option<String>) {
        let text = match result {
            Ok(text) => text,
            Err(why) => {
                let why = self.hide(&why);
                return (Err(Refusal::new(Code::Failed, why.clone())), Some(why));
            }
        };
        match hear(&text) {
            Ok(value) => (Ok(value), None),
            Err(refusal) => {
                let logged = format!("refused: {:?}, {} bytes", refusal.code, refusal.message.len());
                (Err(Refusal::new(refusal.code, self.hide(&refusal.message))), Some(logged))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum Then {
    Client(u64),
    Check(Checking),
    Dialog(u64),
    Clone(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Sent {
    pub(in crate::app) machine: String,
    pub(in crate::app) link: u64,
    pub(in crate::app) boot: Option<String>,
    picks: u64,
}

#[derive(Debug)]
pub(crate) struct Relayed {
    then: Then,
    sent: Sent,
    result: Result<Value, Refusal>,
}

#[cfg(test)]
impl Relayed {
    #[cfg(unix)]
    pub(in crate::app) fn result(&self) -> &Result<Value, Refusal> {
        &self.result
    }
}

impl App {
    pub(in crate::app) fn spawn_ask(
        &self,
        job: Job,
        asking: Asking,
        command: Command,
        told: impl FnOnce(Result<String, String>, &Asking, &mut Job) -> AppEvent + Send + 'static,
    ) {
        let connect = self.machines.connector(&asking.destination);
        let (there, boot, timeout) = (asking.machine.clone(), asking.boot.clone(), asking.timeout);
        let work = move || Ok(ask::ask(connect.as_ref(), &there, command, boot, timeout));
        self.spawn_job(job.slow_after(timeout), work, move |result, job| {
            let result = result.map_err(|error| error.to_string()).and_then(std::convert::identity);
            Some(told(result, &asking, job))
        });
    }

    pub(in crate::app) fn send_to_machine(
        &mut self,
        name: &str,
        send: send::Send,
        then: Then,
    ) -> Result<Sent, Refusal> {
        self.follow_machines();
        let held = self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(name));
        let Some(held) = held else { return Err(unknown(name)) };
        if held.status.state != State::Linked {
            return Err(Refusal::new(Code::Unavailable, self.unlinked(held)));
        }
        let boot = held.seen.as_ref().map(|seen| seen.boot.clone());
        if boot.is_none() && send.focus() {
            let why = format!("{} has not reported yet; try again", held.name);
            return Err(Refusal::new(Code::Unavailable, why));
        }
        let picks = self.machines.picks;
        let target = Sent { machine: held.name.clone(), link: held.link.id(), boot: boot.clone(), picks };
        let awaited = target.clone();
        let asking =
            Asking { machine: held.name.clone(), destination: held.destination.clone(), boot, timeout: send.timeout() };
        let job = Job::new(Level::Info, "machines", "relay").with("name", &asking.machine).with("verb", send.verb());
        let hear = send.hearing();
        self.spawn_ask(send.logged(job), asking, send.command(), move |result, asking, job| {
            let result = asking.heard_as(result, job, hear);
            AppEvent::Machine(MachineEvent::Relayed(Relayed { then, sent: awaited, result }))
        });
        Ok(target)
    }

    pub(in crate::app) fn await_sent(&mut self, sent: &Sent, made: Option<RemoteItem>) {
        if self.machines.picks != sent.picks {
            log::info!("machines", "picked before the answer", name = sent.machine);
            return;
        }
        match (made, sent.boot.clone()) {
            (Some(made), Some(boot)) => self.await_remote(sent.link, boot, made),
            _ => log::info!("machines", "nothing to select", name = sent.machine),
        }
    }

    pub(super) fn relayed(&mut self, relayed: Relayed) {
        let Relayed { then, sent, result } = relayed;
        match then {
            Then::Client(key) => self.relay_answered(key, result),
            Then::Check(checking) => self.check_heard(checking, result),
            Then::Dialog(key) => self.dialog_answered(&sent, key, result),
            Then::Clone(key) => self.clone_relayed(key, result),
        }
    }
}
