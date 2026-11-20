use serde_json::Value;

use super::act::{self, QUICK};
use super::ask;
use super::link::{self, Connect};
use crate::control::{
    Command, FlowImport, FlowInfo, FlowList, FlowSpec, FlowSyncFailure, Flows, MachineFlows, Response,
};
use crate::flows::store::same_name;
use crate::flows::sync::{self, Held};
use crate::time::Stamp;

const USER: &str = "user";
const YOURS: &str = "yours";
const SENDER_KEPT: usize = 64;

#[derive(Debug, Clone, Default)]
pub(crate) struct Sending {
    pub(crate) flows: Vec<Held<FlowSpec>>,
    pub(crate) texts: Vec<(String, String)>,
    pub(crate) left_out: Vec<FlowSyncFailure>,
    pub(crate) from: Option<String>,
}

pub(crate) fn sender(text: &str) -> String {
    link::clean_within(text, SENDER_KEPT).trim().to_string()
}

impl Sending {
    fn import(&self, name: &str) -> Result<Command, String> {
        let text = self.texts.iter().find(|(named, _)| same_name(named, name)).map(|(_, text)| text.clone());
        Ok(Command::FlowImport(FlowImport {
            text: text.ok_or("it could not be read here")?,
            place: YOURS.into(),
            replace: true,
            synced_from: Some(self.from.clone().unwrap_or_default()),
            ..FlowImport::default()
        }))
    }
}

pub(crate) fn sync(connect: &dyn Connect, machine: &str, sending: &Sending) -> MachineFlows {
    let mut report =
        MachineFlows { machine: machine.into(), failed: sending.left_out.clone(), ..MachineFlows::default() };
    let theirs = match listed(connect, machine) {
        Ok(theirs) => theirs,
        Err(why) => {
            report.error = Some(why);
            return report;
        }
    };
    let plan = sync::plan(&sending.flows, &theirs);
    report.same = plan.same.len();
    report.kept = plan.kept;
    for name in plan.send {
        match sending.import(&name).and_then(|command| told(connect, machine, command)) {
            Ok(()) => report.sent.push(name),
            Err(error) if error.contains(sync::NEWER_HERE) => report.kept.push(name),
            Err(error) => report.failed.push(FlowSyncFailure { name, error }),
        }
    }
    report
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

fn listed(connect: &dyn Connect, machine: &str) -> Result<Vec<Held<FlowSpec>>, String> {
    let text = ask::ask(connect, machine, Command::FlowList(FlowList { project: None }), None, QUICK)?;
    let value = answered(&text)?;
    let flows: Flows = serde_json::from_value(value).map_err(|_| act::UNREADABLE.to_string())?;
    let held = |info: FlowInfo| Held {
        changed: info.changed.as_deref().and_then(Stamp::parse),
        name: info.name,
        spec: info.flow,
    };
    Ok(flows.flows.into_iter().filter(|info| info.origin == USER).map(held).collect())
}

#[cfg(test)]
mod tests;
