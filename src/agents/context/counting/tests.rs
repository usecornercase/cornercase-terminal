use std::time::{Duration, UNIX_EPOCH};

use super::*;

pub(crate) fn since() -> SystemTime {
    crate::time::Stamp::parse("2026-10-10T12:00:01.000Z").expect("time")
}
pub(crate) fn sum(counted: Counted) -> (usize, Tokens) {
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
    let records: Vec<_> = (0..600).map(|i| Record { at: since(), id: i.to_string(), usage: Usage::Prompt }).collect();
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
            [Record { at: since(), id: "one".into(), usage: Usage::Completed(Tokens { out, ..Tokens::default() }) }],
        );
    }
    assert_eq!(sum(ledger.take()).1.out, 10);
}

#[test]
fn evicted_conversations_keep_only_cutoffs_and_their_cumulative_baseline() {
    let mut ledger = Ledger::default();
    let record = |usage| Record { at: since(), id: "one".into(), usage };
    let tokens = Tokens { out: 10, ..Tokens::default() };
    let all_three = || {
        [
            record(Usage::Prompt),
            record(Usage::Tokens(tokens)),
            record(Usage::Cumulative { total: tokens, last: tokens }),
        ]
    };
    for i in 0..20 {
        ledger.apply(&i.to_string(), since(), all_three());
    }
    assert_eq!(ledger.conversations.len(), CONVERSATIONS);
    assert_eq!(ledger.forgotten.len(), 4);
    assert_eq!(sum(ledger.take()), (20, Tokens { out: 400, ..Tokens::default() }));
    ledger.apply("0", since(), all_three());
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
