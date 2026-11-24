use std::path::Path;

use super::{answered, told};
use crate::control::persona::{
    PersonaImport, PersonaList, PersonaNotSent, PersonaShow, PersonaText, Personas, PersonasSent,
};
use crate::control::{Command, FlowList, FlowSpec, Flows};
use crate::flows::persona;
use crate::flows::store::same_name;
use crate::machines::act::{QUICK, UNREADABLE};
use crate::machines::ask;
use crate::machines::link::Connect;

pub(crate) const OLDER: &str = "it runs a cornercase without personas: update it there, then send them again";
const NOT_HERE: &str = "it is not among this computer's personas";

enum Outcome {
    Sent,
    Same,
    Differs,
}

pub(crate) fn named<'a>(specs: impl IntoIterator<Item = &'a FlowSpec>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for spec in specs {
        let roles = std::iter::once(&spec.orchestrator).chain(&spec.roles);
        for id in roles.filter_map(|role| role.persona.as_deref()).map(str::trim).filter(|id| !id.is_empty()) {
            if !ids.iter().any(|known| known == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

pub(crate) fn library(dir: &Path, ids: &[String]) -> Vec<(String, String)> {
    ids.iter().filter_map(|id| Some((id.clone(), persona::read(dir, id).ok()?.text))).collect()
}

pub(crate) fn send(connect: &dyn Connect, machine: &str, ids: &[String], library: &[(String, String)]) -> PersonasSent {
    let mut report = PersonasSent::default();
    let there = match listed(connect, machine) {
        Ok(there) => there,
        Err(why) => {
            report.error = Some(why);
            return report;
        }
    };
    for id in ids {
        match one(connect, machine, (id, library), &there) {
            Ok(Outcome::Sent) => report.sent.push(id.clone()),
            Ok(Outcome::Same) => report.same += 1,
            Ok(Outcome::Differs) => report.differ.push(id.clone()),
            Err(error) => report.failed.push(PersonaNotSent { id: id.clone(), error }),
        }
    }
    report
}

pub(crate) fn for_run(
    connect: &dyn Connect,
    machine: &str,
    (name, project): (&str, Option<u64>),
    dir: &Path,
) -> Option<PersonasSent> {
    let text = ask::ask(connect, machine, Command::FlowList(FlowList { project }), None, QUICK).ok()?;
    let flows: Flows = serde_json::from_value(answered(&text).ok()?).ok()?;
    let flow = flows.flows.into_iter().find(|info| same_name(&info.name, name))?;
    let ids = named([&flow.flow]);
    if ids.is_empty() {
        return None;
    }
    Some(send(connect, machine, &ids, &library(dir, &ids)))
}

fn one(
    connect: &dyn Connect,
    machine: &str,
    (id, library): (&str, &[(String, String)]),
    there: &[String],
) -> Result<Outcome, String> {
    let text = library.iter().find(|(known, _)| known == id).map(|(_, text)| text).ok_or(NOT_HERE)?;
    if !there.iter().any(|known| known == id) {
        let file = format!("{id}.{}", persona::EXTENSION);
        let personas = vec![PersonaText { file, text: text.clone(), source: None }];
        told(connect, machine, Command::PersonaImport(PersonaImport { personas, ..PersonaImport::default() }))?;
        return Ok(Outcome::Sent);
    }
    let show = Command::PersonaShow(PersonaShow { id: id.to_string() });
    let shown = answered(&ask::ask(connect, machine, show, None, QUICK)?)?;
    let theirs = shown["text"].as_str().unwrap_or_default();
    Ok(if bare(theirs) == bare(text) { Outcome::Same } else { Outcome::Differs })
}

fn bare(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

fn listed(connect: &dyn Connect, machine: &str) -> Result<Vec<String>, String> {
    let text = ask::ask(connect, machine, Command::PersonaList(PersonaList::default()), None, QUICK)?;
    let personas: Personas = serde_json::from_value(answered(&text)?).map_err(|_| UNREADABLE.to_string())?;
    Ok(personas.personas.into_iter().map(|info| info.id).collect())
}
