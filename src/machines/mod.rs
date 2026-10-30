use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::text;

pub(crate) const ICONS: [(char, &str); 6] =
    [('☁', "server"), ('▭', "laptop"), ('▣', "desktop"), ('◫', "mini"), ('⌂', "home server"), ('◆', "other")];

const NAME_MAX: usize = 20;
const DESTINATION_MAX: usize = 255;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Machine {
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) destination: String,
    pub(crate) folder: String,
}

impl<'de> Deserialize<'de> for Machine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let entry = Value::deserialize(deserializer)?;
        let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
        Ok(Self { name: text("name"), icon: text("icon"), destination: text("destination"), folder: text("folder") })
    }
}

impl Machine {
    fn problem(&self, others: &[Self]) -> Option<String> {
        let shape = valid_name(&self.name).and(valid_destination(&self.destination)).and(valid_folder(&self.folder));
        match shape {
            Err(why) => Some(why.to_string()),
            Ok(()) => name_taken(others, &self.name).or_else(|| destination_taken(others, &self.destination)),
        }
    }
}

pub(crate) fn problems(machines: &[Machine]) -> Vec<Option<String>> {
    machines.iter().enumerate().map(|(at, machine)| machine.problem(&machines[..at])).collect()
}

pub(crate) fn known_icon(icon: &str) -> char {
    ICONS.iter().map(|(glyph, _)| *glyph).find(|glyph| icon.chars().eq([*glyph])).unwrap_or(ICONS[0].0)
}

pub(crate) fn valid_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("the name is required");
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    if name.chars().count() > NAME_MAX || !name.chars().all(allowed) {
        return Err("use letters, digits, `.`, `_` and `-`, at most 20 characters");
    }
    Ok(())
}

pub(crate) fn valid_destination(destination: &str) -> Result<(), &'static str> {
    if destination.is_empty() {
        return Err("the destination is required");
    }
    if destination.starts_with('-') {
        return Err("a destination cannot start with `-`");
    }
    if destination.chars().any(|c| c.is_whitespace() || text::hidden(c)) {
        return Err("a destination cannot hold spaces, control or invisible characters");
    }
    if destination.len() > DESTINATION_MAX {
        return Err("a destination is at most 255 bytes");
    }
    Ok(())
}

pub(crate) fn valid_folder(folder: &str) -> Result<(), &'static str> {
    if folder.is_empty() {
        return Err("the folder is required");
    }
    if folder.chars().any(text::hidden) {
        return Err("a folder cannot hold control or invisible characters");
    }
    if !(folder.starts_with('/') || folder == "~" || folder.starts_with("~/")) {
        return Err("use an absolute path or one starting with ~/");
    }
    Ok(())
}

pub(crate) fn name_taken(others: &[Machine], name: &str) -> Option<String> {
    others
        .iter()
        .find(|other| other.name.eq_ignore_ascii_case(name))
        .map(|other| format!("another machine is already called `{}`", other.name))
}

pub(crate) fn destination_taken(others: &[Machine], destination: &str) -> Option<String> {
    others
        .iter()
        .find(|other| other.destination == destination)
        .map(|other| format!("the machine `{}` already uses this destination", other.name))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    #[default]
    Connecting,
    Linked,
    Unreachable,
    Older,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Info {
    pub(crate) name: String,
    pub(crate) icon: String,
    pub(crate) destination: String,
    pub(crate) folder: String,
    pub(crate) state: State,
    pub(crate) version: Option<String>,
    pub(crate) server_version: Option<String>,
    pub(crate) seen: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) updating: bool,
    pub(crate) problem: Option<String>,
}

#[cfg(test)]
mod tests;
