use serde::{Deserialize, Serialize};

use super::{Entry, State, is_false};
use crate::agents::Known;
use crate::agents::activity::Status;
use crate::agents::context::{Pointer, Said};
use crate::text;

pub(crate) const TEXT_CAP: usize = 4096;
pub(crate) const CLOSING: &str =
    "When you finish, end your last message with a short summary of what you did and what is left to do.";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Pull {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) state: String,
    pub(crate) checks: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Outcome {
    pub(crate) at: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) text: String,
    #[serde(skip_serializing_if = "is_false")]
    pub(crate) cut: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) written: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", deserialize_with = "known_record")]
    pub(crate) record: Option<Pointer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pull_request: Option<Pull>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Taken {
    pub(crate) agent: Option<String>,
    pub(crate) record: Option<Pointer>,
    pub(crate) branch: Option<String>,
    pub(crate) pull_request: Option<Pull>,
}

fn known_record<'de, D: serde::Deserializer<'de>>(from: D) -> Result<Option<Pointer>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(from)?;
    Ok(value.and_then(|value| Pointer::deserialize(value).ok()))
}

impl Outcome {
    pub(crate) fn without_record(&self) -> Self {
        Self { record: None, ..self.clone() }
    }
}

pub(crate) fn cap(text: &str) -> (String, bool) {
    let text = text.trim();
    let kept = text::within_bytes(text, TEXT_CAP);
    (kept.to_string(), kept.len() < text.len())
}

pub(crate) fn closing(prompt: &str) -> String {
    format!("{}\n\n{CLOSING}", prompt.trim_end())
}

pub(crate) fn turn_ended(before: Option<Status>, now: Option<Status>) -> bool {
    matches!(before, Some(Status::Working | Status::Waiting)) && !matches!(now, Some(Status::Working | Status::Waiting))
}

impl Outcome {
    pub(crate) fn captured(previous: Option<Self>, taken: Taken, said: Option<Said>, at: i64) -> Self {
        let previous = previous.unwrap_or_default();
        let (text, cut, written) = match said {
            Some(said) => {
                let (text, cut) = cap(&said.text);
                (text, cut, said.written)
            }
            None => (previous.text, previous.cut, previous.written),
        };
        let moved = matches!((&taken.branch, &previous.branch), (Some(new), Some(old)) if new != old);
        Self {
            at,
            text,
            cut,
            written,
            agent: taken.agent.or(previous.agent),
            record: taken.record.or(previous.record),
            branch: taken.branch.or(previous.branch),
            pull_request: taken.pull_request.or(previous.pull_request.filter(|_| !moved)),
        }
    }
}

fn agent_of(entry: &Entry, outcome: &Outcome) -> String {
    outcome.agent.clone().unwrap_or_else(|| entry.task.agent.clone())
}

fn about_the_message(entry: &Entry, outcome: &Outcome, readable: bool) -> Option<String> {
    let agent = agent_of(entry, outcome);
    if !outcome.text.is_empty() {
        let gone = outcome.record.is_some() && !readable;
        return gone.then(|| format!("{agent} no longer has this conversation's record"));
    }
    if Known::of(&agent).is_none() {
        return Some(format!(
            "{agent} keeps no record cornercase can read, so its result is the branch and pull request only"
        ));
    }
    Some(match outcome.record {
        None => format!("{agent}'s record was not found when its result was kept"),
        Some(_) => format!("{agent} had written no message when its result was kept"),
    })
}

fn without_a_result(entry: &Entry) -> String {
    if entry.is_routine() {
        return "a routine: each of its runs keeps a result of its own".into();
    }
    match &entry.state {
        State::Pending | State::Starting => "it has not started".into(),
        State::Failed { reason } => format!("it did not start: {reason}"),
        State::Missed => "it did not start: it was missed".into(),
        State::Started { .. } | State::Kept { .. } | State::Removed => {
            "no result was kept: its agent did not finish a turn while cornercase watched it".into()
        }
    }
}

pub(crate) fn note(entry: &Entry, readable: bool) -> Option<String> {
    match &entry.result {
        Some(outcome) => about_the_message(entry, outcome, readable),
        None => Some(without_a_result(entry)),
    }
}

#[cfg(test)]
mod tests;
