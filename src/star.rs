use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::persist;

pub(crate) const URL: &str = "https://github.com/usecornercase/cornercase-terminal";

pub(crate) const FIRST_AFTER_DAYS: u32 = 14;

const DAY: i64 = 86_400;

pub(crate) const LATER_FOR: i64 = 90 * DAY;

pub(crate) const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Answer {
    Later,
    Starred,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    pub(crate) version: u32,
    pub(crate) since: Option<i64>,
    pub(crate) days: u32,
    pub(crate) day: Option<i64>,
    pub(crate) shown: Option<i64>,
    pub(crate) answer: Option<Answer>,
}

impl Default for Saved {
    fn default() -> Self {
        Self { version: VERSION, since: None, days: 0, day: None, shown: None, answer: None }
    }
}

impl Saved {
    pub(crate) fn count(&mut self, clock: i64) -> bool {
        let today = clock.div_euclid(DAY);
        if self.day.is_some_and(|day| today <= day) {
            return false;
        }
        self.day = Some(today);
        self.days = self.days.saturating_add(1);
        self.since.get_or_insert(clock);
        true
    }

    pub(crate) fn due(&self, clock: i64) -> bool {
        match (self.answer, self.shown) {
            (Some(Answer::Starred | Answer::Never), _) => false,
            (_, Some(shown)) => clock >= shown.saturating_add(LATER_FOR),
            (_, None) => self.days >= FIRST_AFTER_DAYS,
        }
    }

    pub(crate) fn shown(&mut self, clock: i64) {
        self.shown = Some(clock);
    }

    pub(crate) fn answered(&mut self, answer: Answer) {
        self.answer = Some(answer);
    }

    pub(crate) fn settled(&self) -> bool {
        matches!(self.answer, Some(Answer::Starred | Answer::Never))
    }
}

pub(crate) fn load(path: &Path) -> Saved {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Saved>(&text).ok())
        .filter(|saved| saved.version == VERSION)
        .unwrap_or_default()
}

pub(crate) fn save(path: &Path, saved: &Saved) -> std::io::Result<()> {
    persist::save(path, saved)
}

#[cfg(test)]
mod tests;
