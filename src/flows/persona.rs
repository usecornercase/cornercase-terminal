use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::issues::slug;
use crate::persist;

pub(crate) mod source;

pub(crate) const EXTENSION: &str = "md";
pub(crate) const MAX_BYTES: usize = 256 * 1024;
pub(crate) const MAX_ID: usize = 64;
pub(crate) const MAX_IMPORT: usize = 500;
const FENCE: &str = "---";
const FALLBACK: &str = "persona";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Persona {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) emoji: String,
    pub(crate) color: String,
    pub(crate) source: Option<String>,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) personas: Vec<Persona>,
    pub(crate) broken: Vec<(PathBuf, String)>,
}

impl Listing {
    pub(crate) fn find(&self, id: &str) -> Option<&Persona> {
        self.personas.iter().find(|persona| persona.id == id)
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub(crate) fn id_of(file: &str, name: &str) -> String {
    let stem = Path::new(file).file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    [stem, name].into_iter().map(|text| slug(text, MAX_ID)).find(|id| !id.is_empty()).unwrap_or(FALLBACK.into())
}

struct Front<'a> {
    lines: Vec<&'a str>,
    end: usize,
}

fn front(text: &str) -> Option<Front<'_>> {
    let mut lines = text.split_inclusive('\n');
    if lines.next()?.trim_end() != FENCE {
        return None;
    }
    let mut at = text.find('\n')? + 1;
    let mut kept = Vec::new();
    for line in lines {
        at += line.len();
        if line.trim_end() == FENCE {
            return Some(Front { lines: kept, end: at });
        }
        kept.push(line.trim_end_matches(['\r', '\n']));
    }
    None
}

fn unquoted(value: &str) -> String {
    let value = value.trim();
    let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
    if quoted('"') {
        value[1..value.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
    } else if quoted('\'') {
        value[1..value.len() - 1].replace("''", "'")
    } else {
        value.to_string()
    }
}

fn field(front: &Front, key: &str) -> Option<String> {
    let at = front.lines.iter().position(|line| line.split_once(':').is_some_and(|(k, _)| k == key))?;
    let value = front.lines[at].split_once(':').map_or("", |(_, value)| value).trim();
    if !matches!(value, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
        return Some(unquoted(value));
    }
    let block: Vec<&str> = front.lines[at + 1..]
        .iter()
        .take_while(|line| line.is_empty() || line.starts_with([' ', '\t']))
        .map(|line| line.trim())
        .collect();
    Some(block.join(" ").trim().to_string())
}

pub(crate) fn parse(id: &str, text: &str) -> std::result::Result<Persona, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.len() > MAX_BYTES {
        return Err(format!("it is too long for a persona (at most {} KiB)", MAX_BYTES / 1024));
    }
    if !valid_id(id) {
        return Err(format!("`{id}` is not a persona id: use lower-case letters, digits and -"));
    }
    let front = front(text).ok_or("it has no front matter: a `---` block that gives its `name`")?;
    let name = field(&front, "name").map(|name| name.trim().to_string()).unwrap_or_default();
    if name.is_empty() {
        return Err("its front matter gives no `name`".into());
    }
    let read = |key: &str| field(&front, key).unwrap_or_default();
    let persona = Persona {
        id: id.to_string(),
        description: read("description"),
        emoji: read("emoji"),
        color: read("color"),
        source: field(&front, "source").filter(|source| !source.is_empty()),
        text: text.to_string(),
        name,
    };
    if [&persona.name, &persona.description, &persona.emoji, &persona.color]
        .iter()
        .any(|t| t.contains(char::is_control))
    {
        return Err("its name, description, emoji or color contains a control character".into());
    }
    Ok(persona)
}

pub(crate) fn imported(file: &str, text: &str, source: Option<&str>) -> std::result::Result<Persona, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = source.map_or_else(|| text.to_string(), |source| with_source(text, source));
    let read = parse(FALLBACK, &text)?;
    let id = id_of(file, &read.name);
    Ok(Persona { id, ..read })
}

pub(crate) fn is_persona(text: &str) -> bool {
    front(text.strip_prefix('\u{feff}').unwrap_or(text))
        .and_then(|front| field(&front, "name"))
        .is_some_and(|name| !name.trim().is_empty())
}

fn quoted(value: &str) -> String {
    let escaped: String = value
        .chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| if matches!(c, '"' | '\\') { vec!['\\', c] } else { vec![c] })
        .collect();
    format!("\"{escaped}\"")
}

pub(crate) fn with_source(text: &str, source: &str) -> String {
    let Some(front) = front(text) else { return text.to_string() };
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = format!("{FENCE}{newline}");
    for line in front.lines.iter().filter(|line| line.split_once(':').is_none_or(|(k, _)| k != "source")) {
        out.push_str(line);
        out.push_str(newline);
    }
    let _ = write!(out, "source: {}{newline}{FENCE}{newline}", quoted(source));
    out.push_str(&text[front.end..]);
    out
}

pub(crate) fn path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}"))
}

fn files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION) && path.is_file())
        .filter_map(|path| Some((path.file_stem()?.to_str()?.to_string(), path)))
        .filter(|(id, _)| valid_id(id))
        .collect();
    found.sort();
    found
}

pub(crate) fn ids(dir: &Path) -> Vec<String> {
    files(dir).into_iter().map(|(id, _)| id).collect()
}

pub(crate) fn list(dir: &Path) -> Listing {
    let mut listing = Listing::default();
    for (id, path) in files(dir) {
        match read_file(&id, &path) {
            Ok(persona) => listing.personas.push(persona),
            Err(reason) => listing.broken.push((path, reason)),
        }
    }
    listing.personas.sort_by_key(|persona| (persona.name.to_lowercase(), persona.id.clone()));
    listing
}

fn read_file(id: &str, path: &Path) -> std::result::Result<Persona, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse(id, &text)
}

pub(crate) fn read(dir: &Path, id: &str) -> std::result::Result<Persona, String> {
    if !valid_id(id) {
        return Err(format!("`{id}` is not a persona id"));
    }
    read_file(id, &path(dir, id))
}

pub(crate) fn save(dir: &Path, persona: &Persona) -> Result<PathBuf> {
    let path = path(dir, &persona.id);
    persist::write_bytes(&path, persona.text.as_bytes(), persist::Kind::Synced)?;
    Ok(path)
}

pub(crate) fn remove(dir: &Path, id: &str) -> Result<PathBuf> {
    let path = path(dir, id);
    if !valid_id(id) || !path.is_file() {
        return Err(Error::Control(format!("you have no persona `{id}`: `cornercase persona list` names them")));
    }
    std::fs::remove_file(&path)?;
    Ok(path)
}

#[cfg(test)]
mod tests;
