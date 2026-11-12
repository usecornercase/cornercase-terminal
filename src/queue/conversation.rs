use std::collections::VecDeque;
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

struct Heard {
    spoken: Spoken,
    cut: bool,
}

fn heard(record: &Record) -> Result<Vec<Heard>> {
    let newest_first = record.said_newest_first();
    let mut heard: VecDeque<Heard> = VecDeque::new();
    record.said(&mut |mut spoken| {
        let kept = text::within_bytes(&spoken.said.text, MESSAGE_CAP).len();
        let cut = kept < spoken.said.text.len();
        spoken.said.text.truncate(kept);
        heard.push_back(Heard { spoken, cut });
        if !newest_first && heard.len() > MESSAGES + 1 {
            heard.pop_front();
        }
        !newest_first || heard.len() <= MESSAGES
    })?;
    let mut heard = Vec::from(heard);
    if newest_first {
        heard.reverse();
    }
    Ok(heard)
}

fn in_time_order(heard: &mut [Heard]) {
    let stamp = |item: &Heard| item.spoken.said.written.as_deref().and_then(Stamp::parse);
    if heard.iter().all(|item| stamp(item).is_some()) {
        heard.sort_by_key(|item| stamp(item).unwrap_or(SystemTime::UNIX_EPOCH));
    }
}

fn message(item: &Heard) -> Option<Message> {
    let spoken = &item.spoken;
    (!spoken.said.text.trim().is_empty()).then(|| Message {
        role: spoken.role,
        text: spoken.said.text.clone(),
        written: spoken.said.written.clone(),
        cut: item.cut,
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
