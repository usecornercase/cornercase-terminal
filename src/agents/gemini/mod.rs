use crate::agents::activity::{Activity, glyph};

pub(crate) fn gemini(title: &str) -> Option<Activity> {
    match glyph(title.trim())? {
        '✋' => Some(Activity::Waiting),
        '✦' | '⏲' => Some(Activity::Working),
        '◇' => Some(Activity::Idle),
        _ => None,
    }
}

pub(in crate::agents) mod session;

#[cfg(test)]
pub(crate) mod fake;

#[cfg(test)]
mod tests;
