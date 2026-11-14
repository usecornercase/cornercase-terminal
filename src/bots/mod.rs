use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::agents::headless::Access;

pub(crate) mod brief;
pub(crate) mod chat;
pub(crate) mod folder;
pub(crate) mod turn;

pub(crate) const MAX_BOTS: usize = 50;
pub(crate) const NAME_MAX: usize = 40;
pub(crate) const ID_MAX: usize = 32;
pub(crate) const MESSAGE_MAX: usize = 20_000;
pub(crate) const NOTES_CAP: usize = 8 * 1024;
pub(crate) const TURN_TIMEOUT: Duration = Duration::from_mins(15);
pub(crate) const SERVER_STOPS: [&str; 3] = ["kill-server", "restart", "update"];
pub(crate) const LAST_CHARS: usize = 200;
pub(crate) const BOT_ENV: &str = "CORNERCASE_BOT";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Bot {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) agent: String,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) effort: Option<String>,
    #[serde(default)]
    pub(crate) account: Option<String>,
    pub(crate) access: Access,
    pub(crate) created: i64,
    #[serde(default)]
    pub(crate) conversation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameProblem {
    Invalid(String),
    Taken(String),
}

impl NameProblem {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Invalid(text) | Self::Taken(text) => text,
        }
    }
}

pub(crate) fn name_refusal(name: &str, others: &[&Bot]) -> Option<NameProblem> {
    let length = name.chars().count();
    if length == 0 || length > NAME_MAX {
        return Some(NameProblem::Invalid(format!("a bot's name takes 1 to {NAME_MAX} characters, not {length}")));
    }
    if name.chars().any(char::is_control) {
        return Some(NameProblem::Invalid(String::from("a bot's name takes no control characters")));
    }
    let lower = name.to_lowercase();
    others
        .iter()
        .find(|other| other.name.to_lowercase() == lower)
        .map(|other| NameProblem::Taken(format!("there is a bot called {} already", other.name)))
}

pub(crate) fn message_refusal(text: &str) -> Option<String> {
    let count = text.chars().count();
    if text.trim().is_empty() {
        return Some(String::from("the message is empty"));
    }
    (count > MESSAGE_MAX).then(|| format!("the message is {count} characters; a bot takes at most {MESSAGE_MAX}"))
}

pub(crate) fn new_id(name: &str, taken: &[&Bot], on_disk: &dyn Fn(&str) -> bool) -> String {
    let base = match crate::issues::slug(name, ID_MAX) {
        slug if slug.is_empty() => String::from("bot"),
        slug => slug,
    };
    let used = |id: &str| taken.iter().any(|bot| bot.id == id) || on_disk(id);
    let mut id = base.clone();
    let mut n = 2;
    while used(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

pub(crate) fn find<'a>(bots: &'a [&'a Bot], key: &str) -> Option<&'a Bot> {
    let key = key.trim();
    bots.iter()
        .copied()
        .find(|bot| bot.id == key)
        .or_else(|| bots.iter().copied().find(|bot| bot.name.to_lowercase() == key.to_lowercase()))
}

#[cfg(test)]
pub(crate) fn sample(id: &str, name: &str) -> Bot {
    Bot {
        id: id.to_string(),
        name: name.to_string(),
        agent: String::from("claude"),
        model: None,
        effort: None,
        account: None,
        access: Access::Cornercase,
        created: 100,
        conversation: None,
    }
}

#[cfg(test)]
mod tests;
