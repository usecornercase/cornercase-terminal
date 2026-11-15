use std::collections::BTreeMap;
use std::path::PathBuf;

use super::{Handled, json, malformed};
use crate::app::App;
use crate::control::contract::{Code, Refusal};
use crate::control::persona::{
    BrokenPersona, CHECK, PersonaImport, PersonaInfo, PersonaRemove, PersonaRemoved, PersonaText, Personas,
    PersonasImported,
};
use crate::flows::persona::{self, Listing, MAX_IMPORT, Persona};
use crate::flows::share::visible;
use crate::flows::store::same_name;
use crate::flows::{Flow, Role};
use crate::log;
use crate::paths;

impl App {
    pub(in crate::app) fn personas_dir(&self) -> PathBuf {
        paths::personas(self.config_file.path())
    }

    pub(in crate::app) fn persona_ids(&self) -> Vec<String> {
        persona::ids(&self.personas_dir())
    }

    pub(in crate::app) fn run_personas(&self, flow: &Flow) -> Result<BTreeMap<String, String>, String> {
        let dir = self.personas_dir();
        let roles = std::iter::once(&flow.orchestrator).chain(&flow.roles);
        let mut copies = BTreeMap::new();
        for (role, id) in roles.filter_map(|role| Some((role, role.persona()?))) {
            let persona = persona::read(&dir, id).map_err(|e| unreadable(role, id, &e))?;
            copies.insert(persona.id, persona.text);
        }
        Ok(copies)
    }

    pub(super) fn persona_listing(&self) -> serde_json::Value {
        let dir = self.personas_dir();
        let listing = persona::list(&dir);
        let personas = listing
            .personas
            .iter()
            .map(|found| PersonaInfo { path: Some(persona::path(&dir, &found.id)), ..found.into() })
            .collect();
        let broken = listing.broken.into_iter().map(|(path, error)| BrokenPersona { path, error }).collect();
        json(&Personas { dir, personas, broken })
    }

    pub(super) fn persona_import_request(&mut self, import: &PersonaImport) -> Handled {
        self.import_personas(import).map_err(|e| Refusal::new(e.code, visible(&e.message)))
    }

    fn import_personas(&mut self, import: &PersonaImport) -> Handled {
        if import.personas.is_empty() {
            return Err(malformed("nothing to import: give at least one persona"));
        }
        if import.personas.len() > MAX_IMPORT {
            return Err(malformed(format!("one import takes at most {MAX_IMPORT} personas")));
        }
        let dir = self.personas_dir();
        let incoming = incoming(&import.personas)?;
        let replaced = collisions((&persona::list(&dir), &persona::ids(&dir)), &incoming, import.replace)?;
        let mut answer = PersonasImported { check: CHECK.to_string(), ..PersonasImported::default() };
        answer.personas = incoming
            .iter()
            .zip(&replaced)
            .map(|(found, replaced)| PersonaInfo { replaced: *replaced, ..found.into() })
            .collect();
        if import.check {
            return Ok(Some(json(&answer)));
        }
        let paths = persona::save_all(&dir, &incoming)?;
        for (info, path) in answer.personas.iter_mut().zip(paths) {
            info.path = Some(path);
        }
        let bytes: usize = incoming.iter().map(|found| found.text.len()).sum();
        log::info!("flows", "personas imported", count = incoming.len(), bytes = bytes);
        answer.saved = true;
        Ok(Some(json(&answer)))
    }

    pub(super) fn persona_remove_request(&mut self, remove: &PersonaRemove) -> Handled {
        let dir = self.personas_dir();
        let id = remove.id.trim();
        let name = persona::read(&dir, id).map(|found| found.name).unwrap_or_default();
        let path = persona::remove(&dir, id).map_err(|e| Refusal::new(Code::NotFound, visible(&e.to_string())))?;
        log::info!("flows", "persona removed", persona = id);
        Ok(Some(json(&PersonaRemoved { id: id.to_string(), name, path })))
    }
}

fn unreadable(role: &Role, id: &str, reason: &str) -> String {
    visible(&format!("`{}` follows the persona `{id}`, which cannot be read: {reason}", role.name.trim()))
}

fn incoming(texts: &[PersonaText]) -> Result<Vec<Persona>, Refusal> {
    let mut found: Vec<Persona> = Vec::new();
    let mut problems = Vec::new();
    for text in texts {
        let shown = if text.file.is_empty() { "the persona" } else { text.file.as_str() };
        match persona::imported(&text.file, &text.text, text.source.as_deref()) {
            Ok(read) if found.iter().any(|other| other.id == read.id) => {
                problems.push(format!("two of them would be the persona `{}`", read.id));
            }
            Ok(read) if found.iter().any(|other| same_name(&other.name, &read.name)) => {
                problems.push(format!("two of them are named `{}`", read.name));
            }
            Ok(read) => found.push(read),
            Err(e) => problems.push(format!("`{shown}`: {e}")),
        }
    }
    if problems.is_empty() { Ok(found) } else { Err(malformed(problems.join("; "))) }
}

fn collisions(
    (listing, ids): (&Listing, &[String]),
    incoming: &[Persona],
    replace: bool,
) -> Result<Vec<bool>, Refusal> {
    let mut problems = Vec::new();
    let mut replaced = Vec::new();
    for found in incoming {
        let same = ids.contains(&found.id);
        let named = listing.personas.iter().find(|other| other.id != found.id && same_name(&other.name, &found.name));
        if let Some(other) = named {
            problems.push(format!(
                "your persona `{}` is also named `{}`: remove it first with `cornercase persona remove {}`",
                other.id, other.name, other.id
            ));
        } else if same && !replace {
            problems.push(format!("you already have the persona `{}`: pass --replace to overwrite it", found.id));
        }
        replaced.push(same);
    }
    if problems.is_empty() { Ok(replaced) } else { Err(Refusal::new(Code::Conflict, problems.join("; "))) }
}
