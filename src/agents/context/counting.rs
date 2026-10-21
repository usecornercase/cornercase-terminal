use std::collections::HashMap;
use std::time::SystemTime;

use crate::agents::stats::{Counted, Tokens};

const REMEMBER: usize = 512;
const CONVERSATIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::agents) struct Record {
    pub(crate) at: SystemTime,
    pub(crate) id: String,
    pub(crate) usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agents) enum Usage {
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
pub(in crate::agents) struct Ledger {
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

impl Conversation {
    fn count(&mut self, record: Record, since: SystemTime, counted: &mut Counted) {
        let Record { at, id, usage } = record;
        match usage {
            Usage::Cumulative { total, last } => {
                if let Some(delta) = self.count_cumulative(at, total, last)
                    && at >= since
                    && !delta.is_empty()
                {
                    counted.tokens.push((at, delta));
                }
            }
            _ if at < since => {}
            Usage::Prompt => {
                if self.count_prompt(id, at) {
                    counted.prompts.push(at);
                }
            }
            Usage::Tokens(tokens) | Usage::Completed(tokens) => {
                let completed = matches!(usage, Usage::Completed(_));
                if let Some(delta) = self.count_reply(id, at, tokens, completed)
                    && !delta.is_empty()
                {
                    counted.tokens.push((at, delta));
                }
            }
        }
    }

    fn count_cumulative(&mut self, at: SystemTime, total: Tokens, last: Tokens) -> Option<Tokens> {
        if self.cumulative.is_some_and(|(latest, before)| at < latest || (at == latest && total.decreased(before))) {
            return None;
        }
        let delta = match self.cumulative {
            Some((_, before)) if total.decreased(before) => Tokens::default(),
            Some((_, before)) => total.difference(before),
            None => last,
        };
        self.cumulative = Some((at, total));
        Some(delta)
    }

    fn count_prompt(&mut self, id: String, at: SystemTime) -> bool {
        if self.prompts.contains_key(&id)
            || self.prompt_time.is_some_and(|latest| at < latest)
            || self.forgotten_prompt.is_some_and(|forgotten| at <= forgotten)
        {
            return false;
        }
        self.prompts.insert(id, at);
        self.prompt_time = self.prompt_time.max(Some(at));
        self.forgotten_prompt = self.forgotten_prompt.max(retain(&mut self.prompts, |time| *time));
        true
    }

    fn count_reply(&mut self, id: String, at: SystemTime, tokens: Tokens, completed: bool) -> Option<Tokens> {
        let previous = self.replies.get(&id).copied();
        if completed && previous.is_some() {
            return None;
        }
        if previous.is_some_and(|(latest, _)| at < latest)
            || (previous.is_none()
                && (self.reply_time.is_some_and(|latest| at < latest)
                    || self.forgotten_reply.is_some_and(|forgotten| at <= forgotten)))
        {
            return None;
        }
        let before = previous.map_or_else(Tokens::default, |(_, tokens)| tokens);
        let greatest = Tokens {
            input: tokens.input.max(before.input),
            out: tokens.out.max(before.out),
            cache_read: tokens.cache_read.max(before.cache_read),
            cache_write: tokens.cache_write.max(before.cache_write),
        };
        self.replies.insert(id, (at, greatest));
        self.reply_time = self.reply_time.max(Some(at));
        self.forgotten_reply = self.forgotten_reply.max(retain(&mut self.replies, |(time, _)| *time));
        Some(tokens.difference(before))
    }
}

impl Ledger {
    pub(in crate::agents) fn apply(
        &mut self,
        conversation: &str,
        since: SystemTime,
        records: impl IntoIterator<Item = Record>,
    ) {
        let mut records = records.into_iter().peekable();
        if records.peek().is_none() {
            return;
        }
        self.used = self.used.saturating_add(1);
        if !self.conversations.contains_key(conversation) {
            self.open(conversation);
        }
        let Some(state) = self.conversations.get_mut(conversation) else { return };
        state.used = self.used;
        for record in records {
            state.count(record, since, &mut self.counted);
        }
    }

    fn open(&mut self, conversation: &str) {
        if self.conversations.len() >= CONVERSATIONS {
            self.forget_oldest();
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

    fn forget_oldest(&mut self) {
        let oldest = self.conversations.iter().min_by_key(|(_, state)| state.used).map(|(id, _)| id.clone());
        if let Some((id, state)) = oldest.and_then(|id| self.conversations.remove(&id).map(|state| (id, state))) {
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

    pub(in crate::agents) fn take(&mut self) -> Counted {
        std::mem::take(&mut self.counted)
    }
}

pub(in crate::agents) fn timestamp(entry: &serde_json::Value) -> Option<SystemTime> {
    entry["timestamp"].as_str().and_then(crate::time::Stamp::parse)
}

pub(in crate::agents) fn tokens(
    value: &serde_json::Value,
    input: &str,
    output: &str,
    read: &str,
    write: &str,
) -> Tokens {
    Tokens {
        input: value[input].as_u64().unwrap_or(0),
        out: value[output].as_u64().unwrap_or(0),
        cache_read: value[read].as_u64().unwrap_or(0),
        cache_write: value[write].as_u64().unwrap_or(0),
    }
}

#[cfg(test)]
pub(in crate::agents) mod tests;
