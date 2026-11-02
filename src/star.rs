use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::persist;

pub(crate) const URL: &str = "https://github.com/usecornercase/cornercase-terminal";

pub(crate) const ASK_AT: u32 = 5;

pub(crate) const VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Answer {
    Starred,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    pub(crate) prompts: u32,
    pub(crate) shown: Option<i64>,
    pub(crate) answer: Option<Answer>,
}

impl Default for Saved {
    fn default() -> Self {
        Self { version: VERSION, prompts: 0, shown: None, answer: None }
    }
}

impl Saved {
    pub(crate) fn count(&mut self, prompts: usize) -> bool {
        if prompts == 0 || self.asked() || self.prompts >= ASK_AT {
            return false;
        }
        self.prompts = self.prompts.saturating_add(u32::try_from(prompts).unwrap_or(u32::MAX));
        true
    }

    pub(crate) fn due(&self) -> bool {
        !self.asked() && self.prompts >= ASK_AT
    }

    pub(crate) fn asked(&self) -> bool {
        self.shown.is_some() || self.answer.is_some()
    }

    pub(crate) fn shown(&mut self, clock: i64) {
        self.shown.get_or_insert(clock);
    }

    pub(crate) fn answered(&mut self, answer: Answer) {
        self.answer = Some(answer);
    }
}

pub(crate) fn load(path: &Path) -> Saved {
    let read = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str::<Saved>(&text).ok());
    match read {
        Some(saved) if saved.version == VERSION => saved,
        Some(Saved { version: 1, answer, .. }) => Saved { answer, ..Saved::default() },
        _ => Saved::default(),
    }
}

pub(crate) fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    persist::save(path, saved)
}

#[cfg(test)]
mod tests;
