use std::path::Path;

use super::{Listing, Persona, imported, keepable, taken};
use crate::flows::store::same_name;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Offer {
    pub(crate) persona: Option<Persona>,
    pub(crate) replaced: bool,
    pub(crate) problem: Option<String>,
}

pub(crate) struct Incoming<'a> {
    pub(crate) file: &'a str,
    pub(crate) text: &'a str,
    pub(crate) source: Option<&'a str>,
}

pub(crate) fn offers(incoming: &[Incoming<'_>], (listing, dir): (&Listing, &Path)) -> Vec<Offer> {
    let read: Vec<Result<Persona, String>> =
        incoming.iter().map(|one| imported(one.file, one.text, one.source)).collect();
    let personas: Vec<&Persona> = read.iter().filter_map(|one| one.as_ref().ok()).collect();
    read.iter()
        .map(|one| match one {
            Ok(persona) => offer(persona, &personas, (listing, dir)),
            Err(e) => Offer { problem: Some(e.clone()), ..Offer::default() },
        })
        .collect()
}

fn offer(persona: &Persona, all: &[&Persona], (listing, dir): (&Listing, &Path)) -> Offer {
    let problem = twice(persona, all)
        .or_else(|| mine(persona, listing))
        .or_else(|| keepable(dir, std::slice::from_ref(persona)).err());
    Offer { replaced: taken(dir, &persona.id), problem, persona: Some(persona.clone()) }
}

fn twice(persona: &Persona, all: &[&Persona]) -> Option<String> {
    if all.iter().filter(|other| other.id == persona.id).count() > 1 {
        return Some(format!("its id `{}` appears more than once in the source", persona.id));
    }
    let named = all.iter().filter(|other| same_name(&other.name, &persona.name)).count() > 1;
    named.then(|| format!("another persona in the source is also named `{}`", persona.name))
}

fn mine(persona: &Persona, listing: &Listing) -> Option<String> {
    let other =
        listing.personas.iter().find(|other| other.id != persona.id && same_name(&other.name, &persona.name))?;
    Some(format!("your persona `{}` is also named `{}`", other.id, other.name))
}

#[cfg(test)]
mod tests;
