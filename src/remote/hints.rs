use serde_json::Value;

use super::Remote;

const HINT: &str = "cornercase ";
const QUOTED: &str = "`cornercase ";
const STAYS_HERE: [&str; 5] = ["remote", "skill", "server", "proxy", "plugin windows"];
const MACHINE: [&str; 2] = ["there", "here"];

const ON_A_MACHINE: [(&str, &[&str]); 6] = [
    ("start", &[]),
    ("new-workspace", &[]),
    ("focus", &["--pane", "--worktree", "--project", "--view", "--reveal"]),
    ("flow run", &[]),
    ("stats", &[]),
    ("usage", &[]),
];
const LISTS_THE_MACHINES: [&str; 2] = ["status", "status --json"];

enum Hint {
    Kept,
    Becomes(String),
    There,
}

impl Remote {
    pub(crate) fn hinted(&self, text: &str) -> String {
        walked(
            text,
            |action, after| {
                if rewrites(action, after) { Hint::Becomes(self.line(action)) } else { Hint::Kept }
            },
        )
    }

    pub(crate) fn hint_answer(&self, text: &str, value: &mut Value) {
        if text.contains(QUOTED) {
            errors(value, &|text| self.hinted(text));
        }
    }
}

pub(crate) fn on_machine(name: &str, text: &str) -> String {
    walked(text, |action, after| machine_hint(name, action, after))
}

pub(crate) fn errors_on_machine(name: &str, value: &mut Value) {
    errors(value, &|text| on_machine(name, text));
}

fn walked(text: &str, hint: impl Fn(&str, &str) -> Hint) -> String {
    let parts: Vec<&str> = text.split('`').collect();
    let mut out = String::with_capacity(text.len());
    let mut there = false;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            out.push('`');
        }
        if std::mem::take(&mut there) {
            out.push_str(" there");
        }
        let closed = index % 2 == 1 && index + 1 < parts.len();
        let after = parts.get(index + 1).copied().unwrap_or("");
        let verdict = part.strip_prefix(HINT).filter(|_| closed).map_or(Hint::Kept, |action| hint(action, after));
        match verdict {
            Hint::Kept => out.push_str(part),
            Hint::Becomes(line) => out.push_str(&line),
            Hint::There => {
                out.push_str(part);
                there = true;
            }
        }
    }
    out
}

fn errors(value: &mut Value, hinted: &dyn Fn(&str) -> String) {
    match value {
        Value::Object(fields) => fields.iter_mut().for_each(|(key, field)| match field {
            Value::String(text) if key == "error" => *text = hinted(text),
            other => errors(other, hinted),
        }),
        Value::Array(items) => items.iter_mut().for_each(|item| errors(item, hinted)),
        _ => {}
    }
}

fn machine_hint(name: &str, action: &str, after: &str) -> Hint {
    let named = action.split(' ').any(|word| word == "--machine" || word.starts_with("--machine="));
    if names_a_remote(action) || named || names_a_machine(after) || LISTS_THE_MACHINES.contains(&action) {
        return Hint::Kept;
    }
    for (path, refused) in ON_A_MACHINE {
        if is(action, path) && !action.split(' ').any(|word| refused.contains(&word)) {
            let rest = action[path.len()..].trim_start();
            let line = format!("{path} --machine {} {rest}", super::quote_word(name));
            return Hint::Becomes(format!("{HINT}{}", line.trim_end()));
        }
    }
    Hint::There
}

fn rewrites(action: &str, after: &str) -> bool {
    !names_a_remote(action) && !stays_here(action) && !names_a_machine(after)
}

fn names_a_remote(action: &str) -> bool {
    action.split(' ').any(|word| word.starts_with("--remote"))
}

fn stays_here(action: &str) -> bool {
    STAYS_HERE.iter().any(|name| is(action, name))
}

fn is(action: &str, command: &str) -> bool {
    action == command || action.strip_prefix(command).is_some_and(|rest| rest.starts_with(' '))
}

fn names_a_machine(after: &str) -> bool {
    let Some(rest) = after.strip_prefix(' ') else {
        return false;
    };
    MACHINE
        .iter()
        .any(|word| rest.strip_prefix(word).is_some_and(|tail| !tail.chars().next().is_some_and(char::is_alphanumeric)))
}

mod stream;

pub(super) use stream::{Printed, printed, relayed};

#[cfg(test)]
mod tests;
