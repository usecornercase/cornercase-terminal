use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::text;
use link::Down;

pub(crate) mod link;
pub(crate) mod wake;

#[cfg_attr(not(test), expect(dead_code, reason = "the settings and the request show them in the next commits"))]
pub(crate) const ICONS: [(char, &str); 6] =
    [('☁', "server"), ('▭', "laptop"), ('▣', "desktop"), ('◫', "mini"), ('⌂', "home server"), ('◆', "other")];

const NAME_MAX: usize = 20;
const UPDATING: &str = "updating…";
const CONNECTING: &str = "connecting…";
const NOT_SEEN: &str = "not seen yet";
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

#[cfg_attr(not(test), expect(dead_code, reason = "the settings and the request show it in the next commits"))]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) state: State,
    pub(crate) version: Option<String>,
    pub(crate) server_version: Option<String>,
    pub(crate) seen: Option<i64>,
    pub(crate) down: Option<Down>,
    pub(crate) updating: bool,
    pub(crate) update_failed: Option<String>,
}

impl Status {
    pub(crate) fn linked(&mut self, release: String, server_version: String, now: i64) {
        *self = Self {
            state: State::Linked,
            version: Some(release),
            server_version: Some(server_version),
            seen: Some(now),
            updating: self.updating,
            ..Self::default()
        };
    }

    pub(crate) fn down(&mut self, down: Down, now: i64) {
        if self.state == State::Linked {
            self.seen = Some(now);
        }
        if let Down::Older(there) = &down {
            self.state = State::Older;
            self.version.clone_from(there);
            self.server_version = None;
        } else {
            self.state = State::Unreachable;
        }
        self.down = Some(down);
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "the settings show it in the next commits"))]
    pub(crate) fn note(&self, now: i64) -> String {
        if self.updating {
            return UPDATING.into();
        }
        match self.state {
            State::Connecting => CONNECTING.into(),
            State::Linked => {
                let release = self.version.as_deref().unwrap_or_default();
                let running = self.server_version.as_deref().filter(|running| *running != release);
                let note = running.map(|running| format!(" · its server runs {running} until it restarts there"));
                format!("linked · {release}{}", note.unwrap_or_default())
            }
            State::Unreachable => {
                format!("unreachable · {}", self.seen.map_or_else(|| NOT_SEEN.into(), |seen| ago(now - seen)))
            }
            State::Older => self.version.as_ref().map_or_else(|| "older".into(), |there| format!("older · {there}")),
        }
    }
}

fn ago(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..60 => format!("{secs} s"),
        60..3_600 => format!("{} min", secs / 60),
        3_600..86_400 => format!("{} h", secs / 3_600),
        _ => text::plural(usize::try_from(secs / 86_400).unwrap_or(usize::MAX), "day", "days"),
    }
}

pub(crate) fn hide_destination(text: &str, name: &str, destination: &str) -> String {
    let host = destination.rsplit_once('@').map_or(destination, |(_, host)| host);
    [destination, host]
        .into_iter()
        .filter(|hidden| !hidden.is_empty())
        .fold(text.to_string(), |text, hidden| hide_word(&text, hidden, name))
}

fn hide_word(text: &str, hidden: &str, name: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut from = 0;
    for (at, _) in text.match_indices(hidden) {
        let end = at + hidden.len();
        if whole(text, at, end) {
            shown.push_str(&text[from..at]);
            shown.push_str(name);
            from = end;
        }
    }
    shown.push_str(&text[from..]);
    shown
}

fn whole(text: &str, at: usize, end: usize) -> bool {
    let mut after = text[end..].chars();
    let goes_on = match after.next() {
        Some('.') => after.next().is_some_and(in_host),
        next => next.is_some_and(in_host),
    };
    !text[..at].chars().next_back().is_some_and(in_host) && !goes_on
}

fn in_host(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '@' | '.')
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(not(test), expect(dead_code, reason = "the request and the settings build it in the next commits"))]
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
