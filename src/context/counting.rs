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
    entry["timestamp"].as_str().and_then(crate::time::Stamp::parse)
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
pub(super) mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    pub fn since() -> SystemTime {
        crate::time::Stamp::parse("2026-10-10T12:00:01.000Z").expect("time")
    }
    pub fn sum(counted: Counted) -> (usize, Tokens) {
        let mut sum = Tokens::default();
        for (_, t) in counted.tokens {
            sum.input += t.input;
            sum.out += t.out;
            sum.cache_read += t.cache_read;
            sum.cache_write += t.cache_write;
        }
        (counted.prompts.len(), sum)
    }
    #[test]
    fn discarded_ids_are_not_counted_again_and_updates_only_add_the_difference() {
        let mut ledger = Ledger::default();
        let records = (0..600).map(|i| Record {
            at: UNIX_EPOCH + Duration::from_secs(i),
            id: i.to_string(),
            usage: Usage::Tokens(Tokens { out: 1, ..Tokens::default() }),
        });
        let records: Vec<_> = records.collect();
        ledger.apply("a", UNIX_EPOCH, records.clone());
        assert_eq!(sum(ledger.take()).1.out, 600);
        ledger.apply("a", UNIX_EPOCH, records);
        assert_eq!(sum(ledger.take()).1.out, 0);
        let record = Record {
            at: UNIX_EPOCH + Duration::from_secs(599),
            id: "599".into(),
            usage: Usage::Tokens(Tokens { out: 3, ..Tokens::default() }),
        };
        ledger.apply("a", UNIX_EPOCH, [record]);
        assert_eq!(sum(ledger.take()).1.out, 2);
    }
    #[test]
    fn a_shared_timestamp_cannot_make_evicted_ids_count_again() {
        let mut ledger = Ledger::default();
        let records: Vec<_> =
            (0..600).map(|i| Record { at: since(), id: i.to_string(), usage: Usage::Prompt }).collect();
        ledger.apply("a", since(), records.clone());
        assert!(sum(ledger.take()).0 > 0);
        ledger.apply("a", since(), records);
        assert_eq!(sum(ledger.take()).0, 0);
    }
    #[test]
    fn completed_replies_count_once_even_if_the_final_numbers_are_rewritten() {
        let mut ledger = Ledger::default();
        for out in [10, 20] {
            ledger.apply(
                "a",
                since(),
                [Record {
                    at: since(),
                    id: "one".into(),
                    usage: Usage::Completed(Tokens { out, ..Tokens::default() }),
                }],
            );
        }
        assert_eq!(sum(ledger.take()).1.out, 10);
    }

    #[test]
    fn evicted_conversations_keep_only_cutoffs_and_their_cumulative_baseline() {
        let mut ledger = Ledger::default();
        let record = |usage| Record { at: since(), id: "one".into(), usage };
        let tokens = Tokens { out: 10, ..Tokens::default() };
        for i in 0..20 {
            ledger.apply(
                &i.to_string(),
                since(),
                [
                    record(Usage::Prompt),
                    record(Usage::Tokens(tokens)),
                    record(Usage::Cumulative { total: tokens, last: tokens }),
                ],
            );
        }
        assert_eq!(ledger.conversations.len(), CONVERSATIONS);
        assert_eq!(ledger.forgotten.len(), 4);
        assert_eq!(sum(ledger.take()), (20, Tokens { out: 400, ..Tokens::default() }));
        ledger.apply(
            "0",
            since(),
            [
                record(Usage::Prompt),
                record(Usage::Tokens(tokens)),
                record(Usage::Cumulative { total: tokens, last: tokens }),
            ],
        );
        assert_eq!(sum(ledger.take()), (0, Tokens::default()));
        let later = since() + Duration::from_secs(1);
        ledger.apply(
            "0",
            since(),
            [
                Record { at: later, id: "two".into(), usage: Usage::Prompt },
                Record {
                    at: later,
                    id: "two".into(),
                    usage: Usage::Cumulative { total: Tokens { out: 15, ..Tokens::default() }, last: tokens },
                },
            ],
        );
        assert_eq!(sum(ledger.take()), (1, Tokens { out: 5, ..Tokens::default() }));
    }

    #[test]
    fn conversation_switches_keep_their_counts_separate_and_resumes_add_nothing() {
        let mut ledger = Ledger::default();
        let record = Record { at: since(), id: "one".into(), usage: Usage::Prompt };
        ledger.apply("a", since(), [record.clone()]);
        ledger.apply("b", since(), [record.clone()]);
        ledger.apply("a", since(), [record]);
        assert_eq!(sum(ledger.take()).0, 2);
    }
}
