use serde_json::Value;

use super::{Repository, Shared};

const NAME_PARTS: [&str; 5] = ["TOKEN", "KEY", "SECRET", "PASSWORD", "PASS"];
const FLAG_PARTS: [&str; 4] = ["key", "token", "secret", "password"];
const BEARER_MIN: usize = 20;
const ADVICE: &str = "check no secret goes with it";

enum Found {
    Sets(String),
    Passes(String),
    Bearer,
}

impl Found {
    fn describe(&self, place: &str) -> String {
        match self {
            Self::Sets(name) => format!("{place} sets {name}: {ADVICE}"),
            Self::Passes(flag) => format!("{place} passes {flag}: {ADVICE}"),
            Self::Bearer => format!("{place} holds a Bearer token: {ADVICE}"),
        }
    }
}

pub(crate) fn warnings(shared: &Shared) -> Vec<String> {
    let mut found = Vec::new();
    for (place, text) in texts(shared) {
        for item in scan(&text) {
            let warning = item.describe(&place);
            if !found.contains(&warning) {
                found.push(warning);
            }
        }
    }
    found
}

fn texts(shared: &Shared) -> Vec<(String, String)> {
    let mut texts = Vec::new();
    for (key, value) in &shared.settings {
        nested(key, &mut Vec::new(), value, &mut texts);
    }
    for repository in &shared.repositories {
        texts.extend(repository_texts(repository));
    }
    texts
}

fn nested<'a>(key: &str, names: &mut Vec<&'a str>, value: &'a Value, texts: &mut Vec<(String, String)>) {
    let Value::Object(map) = value else {
        let place = if names.is_empty() { key.to_string() } else { format!("{key} of {}", names.join(".")) };
        texts.extend(text_of(value).map(|text| (place, text)));
        return;
    };
    for (name, inner) in map {
        names.push(name);
        nested(key, names, inner, texts);
        names.pop();
    }
}

fn repository_texts(repository: &Repository) -> Vec<(String, String)> {
    let remote = repository.remotes.first().map_or("", String::as_str);
    vec![
        (format!("setup of {remote}"), repository.setup.clone()),
        (format!("teardown of {remote}"), repository.teardown.clone()),
    ]
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let words: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            Some(words.join(" "))
        }
        _ => None,
    }
}

fn scan(text: &str) -> Vec<Found> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut found = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let next = words.get(index + 1).copied().unwrap_or("");
        found.extend(scan_word(word, next));
    }
    found
}

fn scan_word(word: &str, next: &str) -> Option<Found> {
    if word == "Bearer" {
        return (next.chars().count() >= BEARER_MIN).then_some(Found::Bearer);
    }
    if word.starts_with("--") {
        return scan_flag(word, next);
    }
    scan_assignment(word, next)
}

fn scan_flag(word: &str, next: &str) -> Option<Found> {
    let (name, inline) = match word.split_once('=') {
        Some((name, value)) => (name, value),
        None if next.starts_with("--") => (word, ""),
        None => (word, next),
    };
    let lower = name.to_lowercase();
    let sensitive = FLAG_PARTS.iter().any(|part| lower.contains(part));
    (sensitive && has_value(inline)).then(|| Found::Passes(name.to_string()))
}

fn scan_assignment(word: &str, next: &str) -> Option<Found> {
    let at = word.find(['=', ':'])?;
    let (name, rest) = (&word[..at], &word[at + 1..]);
    let value = if rest.is_empty() && word.as_bytes()[at] == b':' { next } else { rest };
    (is_sensitive_name(name) && has_value(value)).then(|| Found::Sets(name.to_string()))
}

fn is_sensitive_name(name: &str) -> bool {
    name.split(|c: char| !c.is_ascii_alphanumeric()).any(|part| NAME_PARTS.contains(&part.to_uppercase().as_str()))
}

fn has_value(value: &str) -> bool {
    let value = value.trim_matches(['"', '\'']);
    !value.is_empty() && !value.starts_with('$')
}

#[cfg(test)]
mod tests;
