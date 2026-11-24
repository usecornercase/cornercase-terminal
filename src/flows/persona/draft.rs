use std::fmt::Write as _;

use super::{FENCE, front, quoted, without_keys};

const OWN_KEYS: [&str; 4] = ["name", "description", "emoji", "source"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Draft {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) emoji: String,
    pub(crate) body: String,
}

pub(crate) fn body(text: &str) -> &str {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = front(text).map_or(text, |front| &text[front.end..]);
    rest.trim_start_matches(['\r', '\n'])
}

pub(crate) fn compose(draft: &Draft, base: Option<&str>) -> String {
    let base = base.map(|text| text.strip_prefix('\u{feff}').unwrap_or(text));
    let kept = base.and_then(front).map(|front| without_keys(&front, &OWN_KEYS)).unwrap_or_default();
    let mut out = format!("{FENCE}\nname: {}\n", quoted(draft.name.trim()));
    for (key, value) in [("description", &draft.description), ("emoji", &draft.emoji)] {
        if !value.trim().is_empty() {
            let _ = writeln!(out, "{key}: {}", quoted(value.trim()));
        }
    }
    for line in kept {
        let _ = writeln!(out, "{line}");
    }
    let body = draft.body.replace("\r\n", "\n");
    let body = body.trim_matches('\n');
    let _ = write!(out, "{FENCE}\n\n{body}\n");
    out
}

#[cfg(test)]
mod tests;
