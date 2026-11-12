use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::files::{append, prune, read};
use super::*;
use crate::test_util::TempDir;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

#[expect(clippy::unnecessary_wraps, reason = "the callback returns an option")]
fn template(pane: u64) -> Option<Entry> {
    let mut entry = Entry::blank("claude");
    entry.conversation = Some(format!("c{pane}"));
    entry.pane = Some(pane);
    Some(entry)
}

fn observed(pane: u64, key: Option<&str>, status: Option<&str>, prompts: &[u64]) -> Observed {
    Observed {
        pane,
        key: key.map(str::to_string),
        conversation: None,
        status: status.map(str::to_string),
        prompts: prompts.iter().map(|secs| at(*secs)).collect(),
    }
}

fn kinds(entries: &[Entry]) -> Vec<(Event, Option<&str>)> {
    entries.iter().map(|entry| (entry.event, entry.to.as_deref())).collect()
}

#[test]
fn the_fixture_line_round_trips() {
    let text = crate::test_util::fixture!("recap/journal-v1.jsonl");
    for line in text.lines() {
        let entry: Entry = serde_json::from_str(line).expect("a journal line");
        let again: Entry = serde_json::from_str(&serde_json::to_string(&entry).expect("json")).expect("again");
        assert_eq!(entry, again);
    }
    let first: Entry = serde_json::from_str(text.lines().next().expect("a line")).expect("a journal line");
    assert_eq!(first.record, Some(Pointer::Claude("/home/u/.claude/projects/-home-u-api/8f1c.jsonl".into())));
    assert_eq!(first.pull_request.map(|pull| pull.number), Some(212));
}

#[test]
fn a_pane_is_seen_once_then_its_prompts_and_status_changes_follow() {
    let mut journal = Journal::default();
    let first = journal.observe(at(100), &[observed(1, Some("k"), Some("working"), &[90])], &mut template);
    assert_eq!(kinds(&first), [(Event::Seen, None), (Event::Prompt, None), (Event::Status, Some("working"))]);
    assert_eq!(first[1].at, Stamp(at(90)).to_string());
    let same = journal.observe(at(101), &[observed(1, Some("k"), Some("working"), &[])], &mut template);
    assert_eq!(same, Vec::<Entry>::new());
    let waiting = journal.observe(at(102), &[observed(1, Some("k"), Some("waiting"), &[])], &mut template);
    assert_eq!(kinds(&waiting), [(Event::Status, Some("waiting"))]);
}

#[test]
fn a_pane_that_changes_conversation_closes_the_old_one_first() {
    let mut journal = Journal::default();
    journal.observe(at(100), &[observed(1, Some("a"), None, &[])], &mut template);
    let switched = journal.observe(at(200), &[observed(1, Some("b"), None, &[])], &mut template);
    assert_eq!(kinds(&switched), [(Event::Closed, None), (Event::Seen, None)]);
}

#[test]
fn a_pane_that_goes_away_is_closed_with_the_place_it_had() {
    let mut journal = Journal::default();
    journal.observe(at(100), &[observed(7, Some("a"), None, &[])], &mut template);
    let gone = journal.observe(at(200), &[], &mut template);
    assert_eq!(kinds(&gone), [(Event::Closed, None)]);
    assert_eq!(gone[0].pane, Some(7));
}

fn without_an_id(pane: u64) -> Option<Entry> {
    template(pane).map(|entry| Entry { conversation: None, ..entry })
}

fn with_an_id(prompts: &[u64]) -> Observed {
    Observed { conversation: Some("c1".into()), ..observed(1, Some("k"), Some("idle"), prompts) }
}

#[test]
fn an_id_known_only_after_the_conversation_was_seen_reaches_the_entries_after_it_without_a_line_of_its_own() {
    let mut journal = Journal::default();
    let first = journal.observe(at(100), &[observed(1, Some("k"), Some("idle"), &[])], &mut without_an_id);

    let arrived = journal.observe(at(101), &[with_an_id(&[])], &mut without_an_id);
    let prompted = journal.observe(at(102), &[with_an_id(&[102])], &mut without_an_id);
    let quiet = journal.observe(at(103), &[observed(1, Some("k"), Some("idle"), &[])], &mut without_an_id);
    let closed = journal.observe(at(104), &[], &mut without_an_id);

    assert_eq!(kinds(&first), [(Event::Seen, None), (Event::Status, Some("idle"))]);
    assert!(first.iter().all(|entry| entry.conversation.is_none()), "{first:?}");
    assert_eq!((arrived, quiet), (Vec::new(), Vec::new()));
    let later: Vec<(Event, Option<&str>)> =
        prompted.iter().chain(&closed).map(|entry| (entry.event, entry.conversation.as_deref())).collect();
    assert_eq!(later, [(Event::Prompt, Some("c1")), (Event::Closed, Some("c1"))]);
}

#[test]
fn a_pane_without_a_conversation_yet_journals_nothing() {
    let mut journal = Journal::default();
    let entries = journal.observe(at(100), &[observed(1, None, Some("working"), &[100])], &mut template);
    assert_eq!(entries, Vec::<Entry>::new());
}

fn entry_at(time: &str) -> Entry {
    let mut entry = Entry::blank("claude");
    entry.at = time.to_string();
    entry
}

#[test]
fn entries_are_appended_to_the_file_of_their_utc_day_and_read_across_days() {
    let dir = TempDir::new();
    let journal = dir.path().join("journal");
    let entries = [
        entry_at("2026-10-31T23:59:00.000Z"),
        entry_at("2026-11-01T00:01:00.000Z"),
        entry_at("2026-11-01T00:02:00.000Z"),
    ];
    append(&journal, &entries).expect("append");
    let names: Vec<String> = std::fs::read_dir(&journal)
        .expect("dir")
        .flatten()
        .map(|item| item.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"2026-10-31.jsonl".to_string()));
    let from = entries[0].seconds().expect("time");
    assert_eq!(read(&journal, from, from + 3600).len(), 3);
    assert_eq!(read(&journal, from, from + 30).len(), 1);
}

#[test]
fn a_broken_last_line_costs_one_entry() {
    let dir = TempDir::new();
    let journal = dir.path().join("journal");
    append(&journal, &[entry_at("2026-10-31T10:00:00.000Z")]).expect("append");
    let file = journal.join("2026-10-31.jsonl");
    let mut text = std::fs::read_to_string(&file).expect("file");
    text.push_str("{\"v\":1,\"at\":\"2026-10-31T11");
    std::fs::write(&file, text).expect("write");
    let from = entry_at("2026-10-31T00:00:00.000Z").seconds().expect("time");
    assert_eq!(read(&journal, from, from + 86_400).len(), 1);
}

#[test]
fn pruning_keeps_thirty_days_and_drops_the_older_ones() {
    let dir = TempDir::new();
    let journal = dir.path().join("journal");
    let now = entry_at("2026-11-04T12:00:00.000Z").seconds().expect("time");
    let day = |ago: i64| Stamp(at(u64::try_from(now - ago * 86_400).expect("positive"))).to_string();
    append(&journal, &[entry_at(&day(30)), entry_at(&day(31)), entry_at(&day(1))]).expect("append");
    assert_eq!(prune(&journal, now, 30), 1);
    let left = std::fs::read_dir(&journal).expect("dir").count();
    assert_eq!(left, 2);
    assert!(!Path::new(&journal).join(format!("{}.jsonl", &day(31)[..10])).exists());
}
