use ratatui::style::{Color, Style};
use ratatui::text::Span;
use serde_json::Value;

use super::link::{clean_path, clean_within};
use super::{HERE, act};
use crate::control::contract::Refusal;
use crate::flows::folder::MAX_FILES;
use crate::text::{self, truncate_right};

pub(crate) const NEEDS_REMOTE: &str = "needs a git remote";
const GAP: &str = "   ";
const SHOWN: usize = 3;
const TEXT: usize = 60;
const FLOW_KEPT: usize = 65_536;
const FLOW_LINES: [&str; 3] = ["prompt", "description", "notes"];
const FLOW_PATHS: [&str; 1] = ["path"];
const FLOW_LISTS: [&str; 2] = ["flows", "hidden"];
const FLOW_CAPPED: [&str; 4] = ["flows", "hidden", "broken", "builtins"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) name: String,
    pub(crate) icon: char,
    pub(crate) link: u64,
    pub(crate) why: Option<String>,
}

impl Target {
    pub(crate) fn enabled(&self) -> bool {
        self.why.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RunOn {
    pub(crate) targets: Vec<Target>,
    pub(crate) chosen: Option<usize>,
    pub(crate) blocked: Option<&'static str>,
    pub(crate) awaiting: Option<u64>,
    pub(crate) lost: Option<String>,
}

impl RunOn {
    pub(crate) fn can_choose(&self, at: usize) -> bool {
        self.blocked.is_none() && self.targets.get(at).is_some_and(Target::enabled)
    }

    pub(crate) fn choose(&mut self, at: Option<usize>) -> bool {
        if at.is_some_and(|at| !self.can_choose(at)) {
            return false;
        }
        self.chosen = at;
        self.lost = None;
        true
    }

    pub(crate) fn elsewhere(&self) -> bool {
        self.chosen.is_some() || self.lost.is_some()
    }

    pub(crate) fn is_chosen(&self, at: Option<usize>) -> bool {
        self.chosen == at && self.lost.is_none()
    }

    pub(crate) fn kept(fresh: Option<Self>, shown: Option<&Self>) -> Option<Self> {
        let Some(shown) = shown else { return fresh };
        let listed = fresh.is_some();
        let mut fresh = fresh.unwrap_or_default();
        fresh.awaiting = shown.awaiting;
        fresh.lost.clone_from(&shown.lost);
        if let Some(target) = shown.machine() {
            fresh.chosen = fresh.targets.iter().position(|there| there.link == target.link);
            if fresh.chosen.is_none() {
                fresh.lost = Some(fresh.lost_why(&target.name));
            }
        }
        (listed || fresh.lost.is_some()).then_some(fresh)
    }

    fn lost_why(&self, name: &str) -> String {
        if self.targets.iter().any(|target| target.name.eq_ignore_ascii_case(name)) {
            format!("{name} changed in Settings; choose it again")
        } else {
            format!("{name} is no longer listed")
        }
    }

    pub(crate) fn machine(&self) -> Option<&Target> {
        self.targets.get(self.chosen?)
    }

    pub(crate) fn spans(&self, dim: Style) -> Vec<Span<'static>> {
        let mark = |chosen: bool| if chosen { '●' } else { '○' };
        let here = self.chosen.is_none() && self.lost.is_none();
        let mut spans = vec![Span::raw(format!("{} {HERE}", mark(here)))];
        for (at, target) in self.targets.iter().enumerate() {
            let text = format!("{GAP}{} {} {}", mark(self.chosen == Some(at)), target.icon, target.name);
            match (&target.why, self.blocked) {
                (Some(why), None) => spans.push(Span::styled(format!("{text} · {why}"), dim)),
                (_, Some(_)) => spans.push(Span::styled(text, dim)),
                (None, None) => spans.push(Span::raw(text)),
            }
        }
        if let Some(blocked) = self.blocked {
            spans.push(Span::styled(format!(" — {blocked}"), dim));
        }
        if let Some(lost) = &self.lost {
            spans.push(Span::styled(format!(" · {lost}"), Style::default().fg(Color::Red)));
        }
        spans
    }
}

pub(crate) fn from_template(name: &str) -> String {
    format!("from {name}'s template (the issue's URL by default)")
}

pub(crate) fn not_there(name: &str) -> String {
    format!("the project is not on {name}")
}

pub(crate) fn clipped(text: &str) -> String {
    let line = text.split(char::is_control).map(|part| part.trim_matches(blank)).filter(|part| !part.is_empty());
    truncate_right(&line.collect::<Vec<_>>().join(" "), TEXT)
}

pub(crate) fn clipped_or(text: &str, or: &str) -> String {
    let said = clipped(text);
    if said.is_empty() { or.to_string() } else { said }
}

pub(crate) fn reason(machine: &str, text: &str) -> String {
    clipped_or(text, &format!("{machine} gave no reason"))
}

pub(crate) fn capped<S: AsRef<str>>(items: impl Iterator<Item = S>) -> Option<String> {
    let mut items = items.filter(|item| has_text(item.as_ref()));
    let shown: Vec<String> = items.by_ref().take(SHOWN).map(|item| clipped(item.as_ref())).collect();
    if shown.is_empty() {
        return None;
    }
    Some(match items.count() {
        0 => shown.join(", "),
        more => format!("{} … and {more} more", shown.join(", ")),
    })
}

pub(crate) fn has_text(text: &str) -> bool {
    !text.chars().all(blank)
}

fn blank(c: char) -> bool {
    c.is_whitespace() || text::hidden(c)
}

pub(crate) fn heard_flows(text: &str) -> Result<Value, Refusal> {
    act::heard_with(text, flows_cleaned)
}

fn flows_cleaned(mut value: Value) -> Value {
    for list in FLOW_CAPPED {
        if let Some(Value::Array(items)) = value.get_mut(list) {
            items.retain(|item| !FLOW_LISTS.contains(&list) || named_whole(item));
            items.truncate(MAX_FILES);
        }
    }
    flow_text(value, flow_line)
}

fn named_whole(item: &Value) -> bool {
    let names = [&item["name"], &item["flow"]["name"]];
    names.into_iter().all(|name| name.as_str().is_none_or(|name| flow_line(name) == name))
}

fn flow_text(value: Value, how: fn(&str) -> String) -> Value {
    match value {
        Value::String(text) => Value::String(how(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(|item| flow_text(item, how)).collect()),
        Value::Object(fields) => Value::Object(
            fields.into_iter().map(|(key, value)| (flow_line(&key), flow_text(value, flow_field(&key)))).collect(),
        ),
        value => value,
    }
}

fn flow_field(key: &str) -> fn(&str) -> String {
    if FLOW_LINES.contains(&key) {
        flow_lines
    } else if FLOW_PATHS.contains(&key) {
        clean_path
    } else {
        flow_line
    }
}

fn flow_line(text: &str) -> String {
    clean_within(text, FLOW_KEPT)
}

fn flow_lines(text: &str) -> String {
    let kept = |c: char| match c {
        '\n' => Some(c),
        '\r' => None,
        c if c.is_whitespace() => Some(' '),
        c => (!text::hidden(c)).then_some(c),
    };
    text.chars().filter_map(kept).take(FLOW_KEPT).collect()
}

#[cfg(test)]
mod tests;
