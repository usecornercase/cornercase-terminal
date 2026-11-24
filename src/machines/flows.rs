use serde_json::Value;

use super::act::{self, QUICK};
use super::ask;
use super::link::{self, Connect};
use crate::control::contract::{FLOW_PERSONAS, FLOW_SYNC};
use crate::control::persona::PersonasSent;
use crate::control::{
    Command, Contract, FlowImport, FlowInfo, FlowList, FlowSpec, FlowSyncFailure, Flows, MachineFlows, Response,
};
use crate::flows::store::same_name;
use crate::flows::sync::{self, Held};
use crate::time::Stamp;

const USER: &str = "user";
const YOURS: &str = "yours";
const SENDER_KEPT: usize = 64;
pub(crate) const OLDER: &str = "it runs an older cornercase: update it there, then sync again";

#[derive(Debug, Clone, Default)]
pub(crate) struct Sending {
    pub(crate) flows: Vec<Held<FlowSpec>>,
    pub(crate) texts: Vec<(String, String)>,
    pub(crate) left_out: Vec<FlowSyncFailure>,
    pub(crate) from: Option<String>,
    pub(crate) personas: Vec<(String, String)>,
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
    let mut report = MachineFlows { machine: machine.into(), ..MachineFlows::default() };
    let (features, theirs) = match offers_sync(connect, machine).and_then(|f| Ok((f, listed(connect, machine)?))) {
        Ok(found) => found,
        Err(why) => {
            report.error = Some(why);
            return report;
        }
    };
    let plan = sync::plan(&sending.flows, &theirs);
    report.same = plan.same.len();
    report.kept = plan.kept;
    report.personas = personas_of(connect, machine, (sending, &plan.send), &features);
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

fn personas_of(
    connect: &dyn Connect,
    machine: &str,
    (sending, names): (&Sending, &[String]),
    features: &[Value],
) -> Option<PersonasSent> {
    let sent = sending.flows.iter().filter(|held| names.contains(&held.name)).map(|held| &held.spec);
    let ids = personas::named(sent);
    if ids.is_empty() {
        return None;
    }
    if !features.iter().any(|feature| feature == FLOW_PERSONAS) {
        return Some(PersonasSent { error: Some(personas::OLDER.into()), ..PersonasSent::default() });
    }
    Some(personas::send(connect, machine, &ids, &sending.personas))
}

fn offers_sync(connect: &dyn Connect, machine: &str) -> Result<Vec<Value>, String> {
    let mut described = answered(&ask::ask(connect, machine, Command::Contract(Contract {}), None, QUICK)?)?;
    let features = match described["features"].take() {
        Value::Array(features) => features,
        _ => Vec::new(),
    };
    if features.iter().any(|feature| feature == FLOW_SYNC) { Ok(features) } else { Err(OLDER.into()) }
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

pub(crate) mod personas;

#[cfg(test)]
mod tests;
