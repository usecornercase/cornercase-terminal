use std::fmt::Write as _;
use std::path::PathBuf;

use super::store::{self, Listing, Origin};
use super::{Flow, ORCHESTRATOR, Role};
use crate::agents::{self, Known};

pub(crate) const HEADER: &str =
    "# A cornercase flow: import it with `cornercase flow import <file>` or Settings → Agents → flows → import";
const VALUE_WIDTH: usize = 40;
pub(crate) const MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum To {
    Yours,
    Project,
}

pub(crate) fn too_large() -> String {
    format!("too large to share (at most {} KiB)", MAX_BYTES / 1024)
}

pub(crate) fn to_share(flow: &Flow) -> Result<String, String> {
    let text = format!("{HEADER}\n{}", store::to_text(flow)?);
    if text.len() > MAX_BYTES { Err(too_large()) } else { Ok(text) }
}

pub(crate) fn from_share(text: &str) -> Result<Flow, String> {
    if text.len() > MAX_BYTES {
        return Err(too_large());
    }
    store::parse(text.strip_prefix('\u{feff}').unwrap_or(text))
}

fn parts(flow: &Flow, values: bool) -> Vec<String> {
    let mut parts = Vec::new();
    for role in std::iter::once(&flow.orchestrator).chain(&flow.roles) {
        if !role.env.is_empty() {
            let sets: Vec<String> = if values {
                role.env
                    .iter()
                    .map(|(key, value)| format!("{key}={}", crate::text::truncate_right(value, VALUE_WIDTH)))
                    .collect()
            } else {
                env_keys(role).iter().map(ToString::to_string).collect()
            };
            parts.push(format!("sets {} for `{}`", sets.join(", "), role.name.trim()));
        }
        if !role.args.is_empty() {
            parts.push(format!("passes {} to `{}`", agents::join_args(&role.args), role.name.trim()));
        }
    }
    parts
}

pub(crate) fn env_keys(role: &Role) -> Vec<&str> {
    role.env.keys().map(String::as_str).collect()
}

pub(crate) fn risks(flow: &Flow, whose: &str) -> Option<String> {
    described(flow, whose, true)
}

pub(crate) fn key_risks(flow: &Flow, whose: &str) -> Option<String> {
    described(flow, whose, false)
}

fn described(flow: &Flow, whose: &str, values: bool) -> Option<String> {
    let parts = parts(flow, values);
    let (last, rest) = parts.split_last()?;
    let parts = if rest.is_empty() { last.clone() } else { format!("{} and {last}", rest.join(", ")) };
    Some(format!("{whose} {parts}; they run without asking, so check them"))
}

pub(crate) fn env_warning(flow: &Flow) -> Option<String> {
    let roles: Vec<String> = std::iter::once(&flow.orchestrator)
        .chain(&flow.roles)
        .filter(|role| !role.env.is_empty())
        .map(|role| format!("{} for `{}`", env_keys(role).join(", "), role.name.trim()))
        .collect();
    (!roles.is_empty()).then(|| format!("it sets {}: check no secret goes with it", roles.join(", ")))
}

pub(crate) fn missing_agents(flow: &Flow, installed: &[Known]) -> Vec<String> {
    std::iter::once(&flow.orchestrator)
        .chain(&flow.roles)
        .filter(|role| Known::of(&role.agent).is_some_and(|kind| !installed.contains(&kind)))
        .map(|role| format!("`{}` runs {}, which is not installed here", role.name.trim(), role.agent))
        .collect()
}

pub(crate) fn summary(flow: &Flow) -> String {
    let mut out = format!("{}\n", flow.name.trim());
    for role in std::iter::once(&flow.orchestrator).chain(&flow.roles) {
        let name = if role.name == ORCHESTRATOR { ORCHESTRATOR } else { role.name.trim() };
        let _ = writeln!(out, "  {name}: {}", role.summary());
    }
    out
}

pub(crate) fn collision(listing: &Listing, name: &str, to: To, replace: bool) -> Result<Option<PathBuf>, String> {
    let named: Vec<_> = listing.named(name).collect();
    let builtin = listing.builtins.iter().any(|built| store::same_name(built, name));
    let name = name.trim();
    match to {
        To::Yours => {
            if let Some(path) =
                named.iter().find_map(|saved| matches!(saved.origin, Origin::User(_)).then(|| saved.origin.path()))
            {
                return if replace {
                    Ok(path.map(PathBuf::from))
                } else {
                    Err(format!(
                        "you already have a flow named `{name}`: pass --replace to overwrite it or --name to import it as another"
                    ))
                };
            }
            if builtin && !replace {
                return Err(format!(
                    "`{name}` is a built-in flow, and yours would take its place: pass --replace to do it or --name to import it as another"
                ));
            }
            Ok(None)
        }
        To::Project => {
            if builtin || named.iter().any(|saved| matches!(saved.origin, Origin::User(_))) {
                return Err(format!(
                    "a flow named `{name}` is already yours or built in, and the project's would never run: pass --name to import it as another"
                ));
            }
            match named.iter().find(|saved| matches!(saved.origin, Origin::Project(_))) {
                Some(saved) if replace => Ok(saved.origin.path().map(PathBuf::from)),
                Some(_) => Err(format!(
                    "this project already has a flow named `{name}`: pass --replace to overwrite it or --name to import it as another"
                )),
                None => Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests;
