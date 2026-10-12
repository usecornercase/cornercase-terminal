use std::collections::HashMap;
use std::time::SystemTime;

use crate::stats::{Counted, Tokens};

const REMEMBER: usize = 512;
const CONVERSATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Record {
    pub at: SystemTime,
    pub id: String,
    pub usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Usage {
    Prompt,
    Tokens(Tokens),
    Completed(Tokens),
    Cumulative { total: Tokens, last: Tokens },
}

#[derive(Debug, Default)]
struct Conversation {
    used: u64,
    prompts: HashMap<String, SystemTime>,
    replies: HashMap<String, (SystemTime, Tokens)>,
    prompt_time: Option<SystemTime>,
    reply_time: Option<SystemTime>,
    cumulative: Option<(SystemTime, Tokens)>,
    forgotten_prompt: Option<SystemTime>,
    forgotten_reply: Option<SystemTime>,
}

#[derive(Debug, Default)]
struct Forgotten {
    prompt: Option<SystemTime>,
    reply: Option<SystemTime>,
    cumulative: Option<(SystemTime, Tokens)>,
}

#[derive(Debug, Default)]
pub(super) struct Ledger {
    conversations: HashMap<String, Conversation>,
    counted: Counted,
    forgotten: HashMap<String, Forgotten>,
    used: u64,
}

fn retain<T>(map: &mut HashMap<String, T>, time: impl Fn(&T) -> SystemTime) -> Option<SystemTime> {
    if map.len() <= REMEMBER {
        return None;
    }
    let (id, at) = map.iter().min_by_key(|(_, value)| time(value)).map(|(id, value)| (id.clone(), time(value)))?;
    map.remove(&id);
    Some(at)
}

impl Ledger {
    pub fn apply(&mut self, conversation: &str, since: SystemTime, records: impl IntoIterator<Item = Record>) {
        let mut records = records.into_iter().peekable();
        if records.peek().is_none() {
            return;
        }
        self.used = self.used.saturating_add(1);
        if !self.conversations.contains_key(conversation) {
            if self.conversations.len() >= CONVERSATIONS {
                let oldest = self.conversations.iter().min_by_key(|(_, state)| state.used).map(|(id, _)| id.clone());
                if let Some((id, state)) = oldest.and_then(|id| self.conversations.remove(&id).map(|state| (id, state)))
                {
                    self.forgotten.insert(
                        id,
                        Forgotten {
                            prompt: state.prompt_time.max(state.forgotten_prompt),
                            reply: state.reply_time.max(state.forgotten_reply),
                            cumulative: state.cumulative,
                        },
                    );
                }
            }
            let forgotten = self.forgotten.remove(conversation).unwrap_or_default();
            self.conversations.insert(
                conversation.into(),
                Conversation {
                    forgotten_prompt: forgotten.prompt,
                    forgotten_reply: forgotten.reply,
                    cumulative: forgotten.cumulative,
                    ..Conversation::default()
                },
            );
        }
        let Some(state) = self.conversations.get_mut(conversation) else { return };
        state.used = self.used;
        for Record { at, id, usage } in records {
            if let Usage::Cumulative { total, last } = usage {
                if state
                    .cumulative
                    .is_some_and(|(latest, before)| at < latest || (at == latest && total.decreased(before)))
                {
                    continue;
                }
                let delta = match state.cumulative {
                    Some((_, before)) if total.decreased(before) => Tokens::default(),
                    Some((_, before)) => total.difference(before),
                    None => last,
                };
                state.cumulative = Some((at, total));
                if at >= since && !delta.is_empty() {
                    self.counted.tokens.push((at, delta));
                }
                continue;
            }
            if at < since {
                continue;
            }
            match usage {
                Usage::Prompt => {
                    if state.prompts.contains_key(&id)
                        || state.prompt_time.is_some_and(|latest| at < latest)
                        || state.forgotten_prompt.is_some_and(|forgotten| at <= forgotten)
                    {
                        continue;
                    }
                    state.prompts.insert(id, at);
                    state.prompt_time = state.prompt_time.max(Some(at));
                    state.forgotten_prompt = state.forgotten_prompt.max(retain(&mut state.prompts, |time| *time));
                    self.counted.prompts.push(at);
                }
                Usage::Tokens(tokens) | Usage::Completed(tokens) => {
                    let previous = state.replies.get(&id).copied();
                    if matches!(usage, Usage::Completed(_)) && previous.is_some() {
                        continue;
                    }
                    if previous.is_some_and(|(latest, _)| at < latest)
                        || (previous.is_none()
                            && (state.reply_time.is_some_and(|latest| at < latest)
                                || state.forgotten_reply.is_some_and(|forgotten| at <= forgotten)))
                    {
                        continue;
                    }
                    let before = previous.map_or_else(Tokens::default, |(_, tokens)| tokens);
                    let delta = tokens.difference(before);
                    let greatest = Tokens {
                        input: tokens.input.max(before.input),
                        out: tokens.out.max(before.out),
                        cache_read: tokens.cache_read.max(before.cache_read),
                        cache_write: tokens.cache_write.max(before.cache_write),
                    };
                    state.replies.insert(id, (at, greatest));
                    state.reply_time = state.reply_time.max(Some(at));
                    state.forgotten_reply = state.forgotten_reply.max(retain(&mut state.replies, |(time, _)| *time));
                    if !delta.is_empty() {
                        self.counted.tokens.push((at, delta));
                    }
                }
                Usage::Cumulative { .. } => {}
            }
        }
    }
    pub fn take(&mut self) -> Counted {
        std::mem::take(&mut self.counted)
    }
}

pub(super) fn timestamp(entry: &serde_json::Value) -> Option<SystemTime> {
    entry["timestamp"].as_str().and_then(crate::log::Stamp::parse)
}

pub(super) fn tokens(value: &serde_json::Value, input: &str, output: &str, read: &str, write: &str) -> Tokens {
    Tokens {
        input: value[input].as_u64().unwrap_or(0),
        out: value[output].as_u64().unwrap_or(0),
        cache_read: value[read].as_u64().unwrap_or(0),
        cache_write: value[write].as_u64().unwrap_or(0),
    }
}

#[cfg(test)]
pub(super) mod tests;
