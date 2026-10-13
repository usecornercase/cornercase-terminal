use crate::agents::activity::{Activity, BRAILLE, glyph};

const ACTION_REQUIRED: [&str; 2] = ["[ ! ] Action Required", "[ . ] Action Required"];

pub(crate) fn codex(title: &str, turn: bool) -> Activity {
    if ACTION_REQUIRED.iter().any(|prefix| title.starts_with(prefix)) {
        Activity::Waiting
    } else if turn || glyph(title).is_some_and(|g| BRAILLE.contains(&g)) {
        Activity::Working
    } else {
        Activity::Idle
    }
}

pub(in crate::agents) mod message;
pub(in crate::agents) mod rollout;
pub(crate) mod usage;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
