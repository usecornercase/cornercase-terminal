use std::fmt::Write as _;

use serde_json::Value;

use super::{failed, heading};

pub(super) fn text(value: &Value) -> String {
    if value.is_null() { "nothing pending\n".to_string() } else { body(value) }
}

pub(super) fn blocks(panes: &[Value]) -> String {
    panes
        .iter()
        .map(|pane| {
            if let Some(failed) = failed(pane) {
                return failed;
            }
            let (kind, id) = heading(pane);
            let Some(source) = pane["source"].as_str() else { return format!("{kind} {id}  nothing pending\n") };
            let agent = pane["agent"].as_str().unwrap_or("unknown");
            format!("{kind} {id}  {agent}  {source}\n{}", body(pane))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn listed(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn body(value: &Value) -> String {
    let source = value["source"].as_str().unwrap_or("unknown");
    match source {
        "question" => listed(&value["questions"]).map(question).collect::<Vec<_>>().join("\n"),
        "permission" => permission(value),
        other => format!("{other}\n"),
    }
}

fn question(asked: &Value) -> String {
    let mut text = asked["header"].as_str().map_or_else(String::new, |header| format!("{header}: "));
    text.push_str(asked["question"].as_str().unwrap_or_default());
    if asked["multi_select"] == true {
        text.push_str(" [multi-select]");
    }
    text.push('\n');
    for option in listed(&asked["options"]) {
        let number = &option["number"];
        let label = option["label"].as_str().unwrap_or_default();
        let _ = match option["description"].as_str() {
            Some(description) => writeln!(text, "  {number}. {label} - {description}"),
            None => writeln!(text, "  {number}. {label}"),
        };
    }
    text
}

fn permission(value: &Value) -> String {
    let tool = value["tool"].as_str().unwrap_or("unknown");
    match value["command"].as_str() {
        Some(command) => format!("permission: {tool}\n{command}\n"),
        None => format!("permission: {tool}\n"),
    }
}

#[cfg(test)]
mod tests;
