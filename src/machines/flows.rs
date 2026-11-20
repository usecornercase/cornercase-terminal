use std::time::SystemTime;

use serde_json::Value;

use super::act::{self, QUICK};
use super::ask;
use super::link::{self, Connect};
use crate::control::{
    Command, FlowDelete, FlowHide, FlowImport, FlowInfo, FlowList, FlowRestore, FlowSpec, FlowSyncFailure, Flows,
    MachineFlows, Response,
};
use crate::flows::store::same_name;
use crate::flows::sync::{self, Held, Op, Ours, Theirs};
use crate::time::Stamp;

const USER: &str = "user";
const YOURS: &str = "yours";

pub(crate) struct Sending {
    pub(crate) ours: Ours<FlowSpec>,
    pub(crate) texts: Vec<(String, String)>,
    pub(crate) from: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Synced {
    pub(crate) report: MachineFlows,
    pub(crate) same: Vec<(String, Option<SystemTime>)>,
    pub(crate) started: Option<SystemTime>,
}

impl Sending {
    fn text(&self, name: &str) -> Option<&str> {
        self.texts.iter().find(|(named, _)| same_name(named, name)).map(|(_, text)| text.as_str())
    }

    fn changed(&self, name: &str) -> Option<SystemTime> {
        self.ours.flows.iter().find(|held| same_name(&held.name, name)).and_then(|held| held.changed)
    }

    fn command(&self, op: &Op) -> Result<Command, String> {
        let name = op.name().to_string();
        Ok(match op {
            Op::Send(_) => Command::FlowImport(FlowImport {
                text: self.text(&name).ok_or("it could not be read here")?.to_string(),
                place: YOURS.into(),
                replace: true,
                synced_from: Some(self.from.clone().unwrap_or_default()),
                ..FlowImport::default()
            }),
            Op::Delete(_) => Command::FlowDelete(FlowDelete { name, place: YOURS.into(), ..FlowDelete::default() }),
            Op::Hide(_) => Command::FlowHide(FlowHide { name }),
            Op::Restore(_) => Command::FlowRestore(FlowRestore { name }),
        })
    }
}

pub(crate) fn sync(connect: &dyn Connect, machine: &str, sending: &Sending) -> Synced {
    let started = SystemTime::now();
    let mut synced =
        Synced { report: MachineFlows { machine: machine.into(), ..MachineFlows::default() }, ..Synced::default() };
    let theirs = match listed(connect, machine) {
        Ok(theirs) => theirs,
        Err(why) => {
            synced.report.error = Some(why);
            return synced;
        }
    };
    let plan = sync::plan(&sending.ours, &theirs);
    synced.report.same = plan.same.len();
    synced.report.kept = plan.kept;
    synced.same = plan.same.iter().map(|name| (name.clone(), sending.changed(name))).collect();
    for op in &plan.ops {
        match sending.command(op).and_then(|command| told(connect, machine, command)) {
            Ok(()) => done(&mut synced, sending, op),
            Err(error) => synced.report.failed.push(FlowSyncFailure { name: op.name().into(), error }),
        }
    }
    synced.started = Some(started);
    synced
}

fn done(synced: &mut Synced, sending: &Sending, op: &Op) {
    let name = op.name().to_string();
    let list = match op {
        Op::Send(_) => {
            synced.same.push((name.clone(), sending.changed(&name)));
            &mut synced.report.sent
        }
        Op::Delete(_) => &mut synced.report.deleted,
        Op::Hide(_) => &mut synced.report.hidden,
        Op::Restore(_) => &mut synced.report.restored,
    };
    list.push(name);
}

fn answered(text: &str) -> Result<Value, String> {
    match serde_json::from_str::<Response>(text) {
        Ok(Response::Ok(value)) => Ok(value),
        Ok(Response::Error(why)) => Err(link::clean(&why)),
        Err(_) => act::heard_with(text, std::convert::identity).map_err(|refusal| link::clean(&refusal.message)),
    }
}

fn told(connect: &dyn Connect, machine: &str, command: Command) -> Result<(), String> {
    answered(&ask::ask(connect, machine, command, None, QUICK)?).map(drop)
}

fn listed(connect: &dyn Connect, machine: &str) -> Result<Theirs<FlowSpec>, String> {
    let text = ask::ask(connect, machine, Command::FlowList(FlowList { project: None }), None, QUICK)?;
    let value = answered(&text)?;
    let flows: Flows = serde_json::from_value(value).map_err(|_| act::UNREADABLE.to_string())?;
    let held = |info: FlowInfo| Held {
        changed: info.changed.as_deref().and_then(Stamp::parse),
        name: info.name,
        spec: info.flow,
    };
    Ok(Theirs {
        flows: flows.flows.into_iter().filter(|info| info.origin == USER).map(held).collect(),
        hidden: flows.hidden.into_iter().map(|info| info.name).collect(),
        builtins: flows.builtins,
    })
}

#[cfg(test)]
mod tests;
