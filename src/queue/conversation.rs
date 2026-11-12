use std::time::SystemTime;

use crate::agents::context::{Record, Role, Spoken};
use crate::error::Result;
use crate::text;
use crate::time::Stamp;

pub(crate) const MESSAGES: usize = 1000;
pub(crate) const MESSAGE_CAP: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) role: Role,
    pub(crate) text: String,
    pub(crate) written: Option<String>,
    pub(crate) cut: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Talk {
    pub(crate) messages: Vec<Message>,
    pub(crate) earlier: bool,
}

fn heard(record: &Record) -> Result<Vec<Spoken>> {
    let newest_first = record.said_newest_first();
    let mut heard = Vec::new();
    record.said(&mut |spoken| {
        heard.push(spoken);
        !newest_first || heard.len() <= MESSAGES
    })?;
    if newest_first {
        heard.reverse();
    }
    Ok(heard)
}

fn in_time_order(heard: &mut [Spoken]) {
    let stamp = |spoken: &Spoken| spoken.said.written.as_deref().and_then(Stamp::parse);
    if heard.iter().all(|spoken| stamp(spoken).is_some()) {
        heard.sort_by_key(|spoken| stamp(spoken).unwrap_or(SystemTime::UNIX_EPOCH));
    }
}

fn message(spoken: &Spoken) -> Option<Message> {
    let kept = text::within_bytes(&spoken.said.text, MESSAGE_CAP);
    (!kept.trim().is_empty()).then(|| Message {
        role: spoken.role,
        text: kept.to_string(),
        written: spoken.said.written.clone(),
        cut: kept.len() < spoken.said.text.len(),
    })
}

pub(crate) fn read(record: &Record) -> Result<Talk> {
    let mut heard = heard(record)?;
    in_time_order(&mut heard);
    let earlier = heard.len() > MESSAGES;
    let skip = heard.len().saturating_sub(MESSAGES);
    let messages = heard.iter().skip(skip).filter_map(message).collect();
    Ok(Talk { messages, earlier })
}

#[cfg(test)]
mod tests;
