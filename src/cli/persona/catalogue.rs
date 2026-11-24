use std::fmt::Write as _;

use serde::Serialize;

use super::{persona_line, skipped, texts, warning};
use crate::cli::{print_json, print_out};
use crate::client::{self, answer, ask};
use crate::control::Command;
use crate::control::persona::{PersonaImport, PersonaInfo, PersonasImported};
use crate::error::Result;
use crate::flows::persona::source::{Division, Found, Gathered, Source};
use crate::flows::share::visible;

const NO_DIVISION: &str = "personas";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Catalogue {
    pub(super) label: String,
    pub(super) page: String,
    pub(super) web: bool,
    pub(super) check: String,
    pub(super) skipped: usize,
    pub(super) divisions: Vec<Division>,
    pub(super) personas: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct Entry {
    pub(super) file: String,
    pub(super) division: Option<String>,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) emoji: String,
    pub(super) color: String,
    pub(super) bytes: usize,
    pub(super) replaced: bool,
    pub(super) problem: Option<String>,
}

pub(super) fn run(source: &Source, gathered: Gathered, json: bool) -> Result<()> {
    client::use_persona_catalogue()?;
    let request = PersonaImport { personas: texts(&gathered.found), check: true, catalogue: true, replace: false };
    let answered: PersonasImported = answer(ask("persona import", Command::PersonaImport(request))?)?;
    let catalogue = catalogue(source, gathered, answered);
    if json { print_json(&catalogue) } else { print_out(&readable(&catalogue)) }
}

pub(super) fn catalogue(source: &Source, gathered: Gathered, answered: PersonasImported) -> Catalogue {
    let personas = gathered.found.into_iter().zip(answered.personas).map(|(found, info)| entry(found, info)).collect();
    Catalogue {
        label: source.label(),
        page: source.page(),
        web: source.is_web(),
        check: warning(source, &answered.check),
        skipped: gathered.skipped,
        divisions: gathered.divisions,
        personas,
    }
}

fn entry(found: Found, info: PersonaInfo) -> Entry {
    Entry {
        file: found.path,
        division: found.division,
        id: info.id,
        name: info.name,
        description: info.description,
        emoji: info.emoji,
        color: info.color,
        bytes: info.bytes,
        replaced: info.replaced,
        problem: info.problem,
    }
}

fn groups(catalogue: &Catalogue) -> Vec<(String, Vec<&Entry>)> {
    let label = |key: &str| {
        catalogue.divisions.iter().find(|d| d.key == key).map_or_else(|| key.to_string(), |d| d.label.clone())
    };
    let mut groups: Vec<(String, Vec<&Entry>)> = Vec::new();
    for entry in &catalogue.personas {
        let name = entry.division.as_deref().map_or_else(|| NO_DIVISION.to_string(), label);
        match groups.iter_mut().find(|(group, _)| *group == name) {
            Some((_, members)) => members.push(entry),
            None => groups.push((name, vec![entry])),
        }
    }
    groups
}

fn entry_lines(entry: &Entry) -> String {
    let Some(problem) = &entry.problem else {
        let info = PersonaInfo {
            id: entry.id.clone(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            emoji: entry.emoji.clone(),
            bytes: entry.bytes,
            replaced: entry.replaced,
            ..PersonaInfo::default()
        };
        return persona_line(&info);
    };
    let what = if entry.id.is_empty() { &entry.file } else { &entry.id };
    format!("{}: cannot be imported: {}\n", visible(what), visible(problem))
}

pub(super) fn readable(catalogue: &Catalogue) -> String {
    let mut out = String::new();
    for (name, members) in groups(catalogue) {
        let _ = writeln!(out, "{} ({})", visible(&name), count(members.len()));
        for entry in members {
            for line in entry_lines(entry).lines() {
                let _ = writeln!(out, "  {line}");
            }
        }
    }
    let _ = writeln!(out, "{}", catalogue.check);
    if catalogue.skipped > 0 {
        let _ = writeln!(out, "skipped {}", skipped(catalogue.skipped));
    }
    let refused = catalogue.personas.iter().filter(|entry| entry.problem.is_some()).count();
    let _ = writeln!(
        out,
        "{} in {}, {refused} of them cannot be imported; nothing was imported",
        count(catalogue.personas.len()),
        visible(&catalogue.label)
    );
    out
}

fn count(n: usize) -> String {
    if n == 1 { "1 persona".into() } else { format!("{n} personas") }
}

#[cfg(test)]
mod tests;
