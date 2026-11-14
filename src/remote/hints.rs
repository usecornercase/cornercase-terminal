use serde_json::Value;

use super::Remote;

const HINT: &str = "cornercase ";
const QUOTED: &str = "`cornercase ";
const STAYS_HERE: [&str; 5] = ["remote", "skill", "server", "proxy", "plugin windows"];
const MACHINE: [&str; 2] = ["there", "here"];

impl Remote {
    pub(crate) fn hinted(&self, text: &str) -> String {
        let parts: Vec<&str> = text.split('`').collect();
        parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let closed = index % 2 == 1 && index + 1 < parts.len();
                let after = parts.get(index + 1).copied().unwrap_or("");
                match part.strip_prefix(HINT) {
                    Some(action) if closed && rewrites(action, after) => self.line(action),
                    _ => (*part).to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("`")
    }

    pub(crate) fn hint_answer(&self, text: &str, value: &mut Value) {
        if text.contains(QUOTED) {
            self.hint_walk(value);
        }
    }

    fn hint_walk(&self, value: &mut Value) {
        match value {
            Value::Object(fields) => fields.iter_mut().for_each(|(key, field)| self.hint_field(key, field)),
            Value::Array(items) => items.iter_mut().for_each(|item| self.hint_walk(item)),
            _ => {}
        }
    }

    fn hint_field(&self, key: &str, field: &mut Value) {
        match field {
            Value::String(text) if key == "error" => *text = self.hinted(text),
            other => self.hint_walk(other),
        }
    }
}

fn rewrites(action: &str, after: &str) -> bool {
    !names_a_remote(action) && !stays_here(action) && !names_a_machine(after)
}

fn names_a_remote(action: &str) -> bool {
    action.split(' ').any(|word| word.starts_with("--remote"))
}

fn stays_here(action: &str) -> bool {
    STAYS_HERE.iter().any(|name| action == *name || action.strip_prefix(name).is_some_and(|rest| rest.starts_with(' ')))
}

fn names_a_machine(after: &str) -> bool {
    let Some(rest) = after.strip_prefix(' ') else {
        return false;
    };
    MACHINE
        .iter()
        .any(|word| rest.strip_prefix(word).is_some_and(|tail| !tail.chars().next().is_some_and(char::is_alphanumeric)))
}

#[cfg(test)]
mod tests;
