use serde::Deserialize;
use serde_json::Value;

use super::Listed;
use crate::queue::clock;
use crate::time::{self, Stamp};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Pull {
    number: u64,
    state: String,
    checks: Option<String>,
    url: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Kept {
    at: i64,
    text: String,
    branch: Option<String>,
    pull_request: Option<Pull>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Said {
    role: String,
    text: String,
    written: Option<String>,
}

pub(super) fn partial(value: &Value, conversation: bool) -> bool {
    conversation && value["messages"].is_null()
}

fn time_of(written: Option<&str>, zone: &str) -> Option<String> {
    let at = Stamp::parse(written?)?;
    Some(clock::display(time::secs(at), zone))
}

fn heading(entry: &Listed) -> String {
    let who = entry.flow.as_ref().map_or_else(|| entry.agent.clone(), |flow| format!("flow {flow}"));
    format!("{} {} · {} · {who}", entry.id, entry.status(), entry.label)
}

fn times(entry: &Listed, kept: Option<&Kept>, zone: &str) -> String {
    let start = match entry.started {
        Some(at) => format!("started {}", clock::display(at, zone)),
        None => format!("scheduled {}", clock::display(entry.at, zone)),
    };
    match kept {
        Some(kept) => format!("{start} · result {}", clock::display(kept.at, zone)),
        None => start,
    }
}

fn pull_line(pull: &Pull) -> String {
    let mut line = format!("pull request #{} {}", pull.number, pull.state);
    if let Some(checks) = &pull.checks {
        line.push_str(&format!(" · checks {checks}"));
    }
    if !pull.url.is_empty() {
        line.push_str(&format!(" · {}", pull.url));
    }
    line
}

fn messages(value: &Value, zone: &str) -> Vec<String> {
    let Some(list) = value["messages"].as_array() else { return Vec::new() };
    let mut out = vec![String::new()];
    if value["earlier"].as_bool() == Some(true) {
        out.push("(earlier messages are left out)".into());
        out.push(String::new());
    }
    for said in list.iter().filter_map(|said| Said::deserialize(said).ok()) {
        match time_of(said.written.as_deref(), zone) {
            Some(at) => out.push(format!("{} · {at}", said.role)),
            None => out.push(said.role),
        }
        out.push(said.text);
        out.push(String::new());
    }
    out
}

pub(super) fn text(value: &Value, zone: &str) -> String {
    let entry = Listed::deserialize(value).unwrap_or_default();
    let kept = serde_json::from_value::<Option<Kept>>(value["result"].clone()).ok().flatten();
    let mut lines = vec![heading(&entry), times(&entry, kept.as_ref(), zone)];
    if let Some(branch) = kept.as_ref().and_then(|kept| kept.branch.as_ref()) {
        lines.push(format!("branch {branch}"));
    }
    if let Some(pull) = kept.as_ref().and_then(|kept| kept.pull_request.as_ref()) {
        lines.push(pull_line(pull));
    }
    lines.push(String::new());
    if let Some(kept) = kept.as_ref().filter(|kept| !kept.text.is_empty()) {
        lines.push(kept.text.clone());
    }
    if let Some(note) = value["note"].as_str() {
        lines.push(note.to_string());
    }
    lines.extend(messages(value, zone));
    lines.join("\n").trim_end().to_string() + "\n"
}
