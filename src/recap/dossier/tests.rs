use std::path::Path;

use super::*;
use crate::recap::journal::{WorkspacePlace, files};
use crate::recap::range::resolve;
use crate::test_util::TempDir;

const MADRID: &str = "Europe/Madrid";

fn seconds(time: &str) -> i64 {
    let mut entry = Entry::blank("claude");
    entry.at = time.to_string();
    entry.seconds().expect("a time")
}

fn transcript(dir: &Path, name: &str, lines: &[(&str, &str, &str)]) -> std::path::PathBuf {
    let path = dir.join(format!("{name}.jsonl"));
    let text: Vec<String> = lines
        .iter()
        .map(|(role, at, text)| {
            let text = serde_json::to_string(text).expect("json");
            if *role == "prompt" {
                format!(
                    r#"{{"type":"user","origin":{{"kind":"human"}},"timestamp":"{at}","message":{{"content":[{{"type":"text","text":{text}}}]}}}}"#
                )
            } else {
                format!(
                    r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"m{at}","model":"claude-opus-5-5","content":[{{"type":"text","text":{text}}}]}}}}"#
                )
            }
        })
        .collect();
    std::fs::write(&path, text.join("\n") + "\n").expect("write the transcript");
    path
}

fn entry(event: Event, at: &str, name: &str, path: &Path) -> Entry {
    let mut entry = Entry::blank("claude");
    entry.at = at.to_string();
    entry.event = event;
    entry.conversation = Some(name.to_string());
    entry.record = Some(Pointer::Claude(path.to_path_buf()));
    entry.pane = Some(3);
    entry.boot = Some("boot".into());
    entry.project = Some(Named { label: "api".into(), path: "/p/api".into() });
    entry.workspace = Some(WorkspacePlace {
        label: "rate-limit".into(),
        branch: Some("feat/rate-limit".into()),
        path: "/gone/worktrees/rate-limit".into(),
        worktree: true,
    });
    entry.tab = Some("claude".into());
    entry
}

fn working(at: &str, name: &str, path: &Path) -> Entry {
    Entry { to: Some("working".into()), ..entry(Event::Status, at, name, path) }
}

fn yesterday(entries: &[Entry]) -> Range {
    resolve(Choice::Yesterday, entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("a day")
}

fn conversation(dir: &Path) -> (std::path::PathBuf, Vec<Entry>) {
    let path = transcript(
        dir,
        "c1",
        &[
            ("prompt", "2026-11-02T10:00:00.000Z", "an older request"),
            ("prompt", "2026-11-03T09:12:00.000Z", "add a rate limit to the public API"),
            ("reply", "2026-11-03T09:30:00.000Z", "Done. Redis or in memory?"),
            ("prompt", "2026-11-03T09:40:00.000Z", "yes"),
            ("prompt", "2026-11-03T10:30:00.000Z", "/compact"),
            ("prompt", "2026-11-03T11:30:00.000Z", "ask before connecting the first time"),
        ],
    );
    let entries = vec![
        entry(Event::Seen, "2026-11-03T09:12:00.000Z", "c1", &path),
        entry(Event::Prompt, "2026-11-03T09:12:00.000Z", "c1", &path),
        working("2026-11-03T09:12:01.000Z", "c1", &path),
        entry(Event::Closed, "2026-11-03T17:02:00.000Z", "c1", &path),
    ];
    (path, entries)
}

#[test]
fn yesterday_lists_the_prompts_of_that_day_without_the_noise() {
    let dir = TempDir::new();
    let (_, entries) = conversation(dir.path());
    let range = yesterday(&entries);
    let dossier = build(&range, &entries, &Live::default()).expect("a dossier");
    assert!(dossier.text.contains("## [c1] api › rate-limit › claude"), "{}", dossier.text);
    assert!(dossier.text.contains("\"add a rate limit to the public API\""));
    assert!(dossier.text.contains("\"ask before connecting the first time\""));
    for left_out in ["an older request", "/compact", "\"yes\""] {
        assert!(!dossier.text.contains(left_out), "{left_out} in {}", dossier.text);
    }
    let first = dossier.text.find("add a rate limit").expect("first");
    assert!(first < dossier.text.find("ask before").expect("second"));
    assert_eq!(dossier.conversations, 1);
}

#[test]
fn a_closed_conversation_keeps_the_place_and_the_branch_from_the_journal() {
    let dir = TempDir::new();
    let (path, entries) = conversation(dir.path());
    let dossier = build(&yesterday(&entries), &entries, &Live::default()).expect("a dossier");
    assert!(dossier.text.contains("branch feat/rate-limit"));
    assert!(dossier.text.contains("worktree /gone/worktrees/rate-limit"));
    assert!(dossier.text.contains("closed at"), "{}", dossier.text);
    assert!(dossier.text.contains(&format!("transcript {}", path.display())));
    let place = &dossier.places["c1"];
    assert_eq!(place.workspace.as_ref().map(|w| w.path.as_str()), Some("/gone/worktrees/rate-limit"));
}

#[test]
fn the_last_reply_is_quoted() {
    let dir = TempDir::new();
    let (_, entries) = conversation(dir.path());
    let dossier = build(&yesterday(&entries), &entries, &Live::default()).expect("a dossier");
    assert!(dossier.text.contains("### Last reply\n> Done. Redis or in memory?"), "{}", dossier.text);
}

#[test]
fn a_waiting_pane_shows_its_question_and_the_last_screen_lines_in_now() {
    let dir = TempDir::new();
    let (path, entries) = conversation(dir.path());
    let template = entry(Event::Seen, "2026-11-03T09:12:00.000Z", "c1", &path);
    let screen: Vec<String> = (1..=20).map(|n| format!("screen line {n}")).collect();
    let live = Live {
        panes: vec![LivePane { key: template.key().expect("a key"), status: Some("waiting".into()), template, screen }],
        ..Live::default()
    };
    let now = seconds("2026-11-04T09:00:00.000Z");
    let range = resolve(Choice::Now, &entries, now, MADRID).expect("now");
    let dossier = build(&range, &[], &live).expect("a dossier");
    assert!(dossier.text.contains("now: waiting"), "{}", dossier.text);
    assert!(dossier.text.contains("### Waiting on"));
    assert!(dossier.text.contains("screen line 20") && dossier.text.contains("screen line 6"));
    assert!(!dossier.text.contains("screen line 5\n"));
    assert_eq!(dossier.places["c1"].pane, Some(3));
    assert_eq!(dossier.places["c1"].status.as_deref(), Some("waiting"));
    assert!(dossier.text.contains("## Needs you now\n- [c1] waits for an answer"), "{}", dossier.text);
}

#[test]
fn now_lists_the_newest_prompts_first_within_the_cap() {
    let dir = TempDir::new();
    let lines: Vec<(&str, String, String)> =
        (1..=9).map(|n| ("prompt", format!("2026-11-04T08:0{n}:00.000Z"), format!("request number {n}"))).collect();
    let borrowed: Vec<(&str, &str, &str)> = lines.iter().map(|(r, a, t)| (*r, a.as_str(), t.as_str())).collect();
    let path = transcript(dir.path(), "c9", &borrowed);
    let template = entry(Event::Seen, "2026-11-04T08:01:00.000Z", "c9", &path);
    let live = Live {
        panes: vec![LivePane {
            key: template.key().expect("key"),
            status: Some("working".into()),
            template,
            screen: vec![],
        }],
        ..Live::default()
    };
    let range = resolve(Choice::Now, &[], seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("now");
    let text = build(&range, &[], &live).expect("a dossier").text;
    assert!(text.find("request number 9").expect("nine") < text.find("request number 6").expect("six"));
    assert!(!text.contains("request number 5"), "{text}");
}

#[test]
fn the_pull_request_and_its_checks_get_an_id() {
    let dir = TempDir::new();
    let (path, mut entries) = conversation(dir.path());
    entries[0].pull_request =
        Some(PullPlace { number: 212, url: "u".into(), state: "open".into(), ci: Some("failing".into()) });
    let mut last = entry(Event::Closed, "2026-11-03T17:02:00.000Z", "c1", &path);
    last.pull_request = entries[0].pull_request.clone();
    entries[3] = last;
    let dossier = build(&yesterday(&entries), &entries, &Live::default()).expect("a dossier");
    assert!(dossier.text.contains("pull request [p1] #212 · open · checks failing"), "{}", dossier.text);
    assert_eq!(dossier.places["p1"].number, Some(212));
}

#[test]
fn a_long_prompt_is_cut_and_a_paste_is_folded() {
    let dir = TempDir::new();
    let long = "x".repeat(900);
    let paste = format!("trace\n{}", "frame\n".repeat(40));
    let path = transcript(
        dir.path(),
        "c2",
        &[("prompt", "2026-11-03T09:00:00.000Z", &long), ("prompt", "2026-11-03T09:10:00.000Z", &paste)],
    );
    let entries = vec![working("2026-11-03T09:00:00.000Z", "c2", &path)];
    let text = build(&yesterday(&entries), &entries, &Live::default()).expect("a dossier").text;
    assert!(text.contains(&format!("\"{}…\"", "x".repeat(300))), "{text}");
    assert!(text.contains("trace (pasted 41 lines)"));
}

#[test]
fn past_the_total_cap_later_conversations_keep_only_their_header() {
    let dir = TempDir::new();
    let mut entries = Vec::new();
    let four_bytes_each = "🦀".repeat(490);
    for n in 0..40 {
        let prompts: Vec<(&str, String, String)> = (0..8)
            .map(|i| ("prompt", format!("2026-11-03T09:{i:02}:00.000Z"), format!("{n}-{i} {four_bytes_each}")))
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = prompts.iter().map(|(r, a, t)| (*r, a.as_str(), t.as_str())).collect();
        let path = transcript(dir.path(), &format!("c{n}"), &borrowed);
        entries.push(working("2026-11-03T09:00:00.000Z", &format!("c{n}"), &path));
    }
    let range = yesterday(&entries);
    let text = build(&range, &entries, &Live::default()).expect("a dossier").text;
    assert!(text.len() < 49 * 1024, "{}", text.len());
    assert!(text.contains("conversations were shortened"), "{}", &text[..300]);
    assert!(text.contains("\"0-7 "), "the first conversation is whole: {}", &text[..300]);
    assert!(text.contains("## [c8] "), "a middle one keeps its header: {}", &text[..300]);
    assert!(!text.contains("\"39-7 "), "the last loses its prompts: {}", &text[..300]);
}

#[test]
fn nothing_in_the_range_gives_no_dossier() {
    let range = resolve(Choice::Week, &[], seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("a week");
    assert!(build(&range, &[], &Live::default()).is_none());
}

#[test]
fn the_week_adds_the_pull_requests_that_were_merged() {
    let dir = TempDir::new();
    let (path, mut entries) = conversation(dir.path());
    let pull = PullPlace { number: 99, url: "u".into(), state: "merged".into(), ci: None };
    entries[3] = Entry { pull_request: Some(pull), ..entry(Event::Closed, "2026-11-03T17:02:00.000Z", "c1", &path) };
    let range = resolve(Choice::Week, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("a week");
    let text = build(&range, &entries, &Live::default()).expect("a dossier").text;
    assert!(text.contains("## Merged pull requests\n- [p1] #99"), "{text}");
    let day = resolve(Choice::Yesterday, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("a day");
    assert!(!build(&day, &entries, &Live::default()).expect("a dossier").text.contains("## Merged"));
}

#[test]
fn entries_survive_a_round_trip_through_the_day_files() {
    let dir = TempDir::new();
    let (_, entries) = conversation(dir.path());
    let journal = dir.path().join("journal");
    files::append(&journal, &entries).expect("append");
    let read = files::read(&journal, seconds("2026-11-03T00:00:00.000Z"), seconds("2026-11-04T00:00:00.000Z"));
    assert_eq!(read.len(), entries.len());
}

fn live_pane(entry: &Entry, status: &str) -> LivePane {
    LivePane { key: entry.key().expect("a key"), status: Some(status.into()), template: entry.clone(), screen: vec![] }
}

fn now_range() -> Range {
    resolve(Choice::Now, &[], seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("now")
}

fn one_conversation(dir: &Path, name: &str) -> (std::path::PathBuf, Entry) {
    let path = transcript(dir, name, &[("prompt", "2026-11-04T08:00:00.000Z", "do the thing")]);
    let template = entry(Event::Seen, "2026-11-04T08:00:00.000Z", name, &path);
    (path, template)
}

#[test]
fn now_lists_what_needs_you_from_open_panes_only() {
    let dir = TempDir::new();
    let (_, waiting) = one_conversation(dir.path(), "w");
    let (closed_path, _) = one_conversation(dir.path(), "x");
    let (_, failed) = one_conversation(dir.path(), "f");
    let entries = vec![
        working("2026-11-04T08:00:00.000Z", "x", &closed_path),
        entry(Event::Closed, "2026-11-04T08:30:00.000Z", "x", &closed_path),
    ];
    let live = Live { panes: vec![live_pane(&waiting, "waiting"), live_pane(&failed, "failed")], ..Live::default() };
    let range = resolve(Choice::Now, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("now");
    let dossier = build(&range, &entries, &live).expect("a dossier");
    let text = &dossier.text;
    assert!(text.find("## Needs you now").expect("list") < text.find("## [c").expect("first"), "{text}");
    let list = &text[text.find("## Needs you now").expect("list")..text.find("## [c1]").expect("c1")];
    assert_eq!(list.matches("\n- [c").count(), 2, "{list}");
    assert!(list.contains("waits for an answer") && list.contains("failed"), "{list}");
    let closed = dossier.places.iter().find(|(_, place)| place.status.is_none()).expect("a closed place");
    assert!(!list.contains(&format!("[{}]", closed.0)), "{list}");
    assert!(dossier.places.values().any(|place| place.status.as_deref() == Some("waiting")));
}

#[test]
fn a_failing_pull_request_needs_you_only_when_read_live() {
    let dir = TempDir::new();
    let (path, mut entries) = conversation(dir.path());
    let pull = PullPlace { number: 212, url: "u".into(), state: "open".into(), ci: Some("failing".into()) };
    entries[3] = Entry { pull_request: Some(pull), ..entry(Event::Closed, "2026-11-03T17:02:00.000Z", "c1", &path) };
    let range = resolve(Choice::Now, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("now");
    let journal_only = build(&range, &entries, &Live::default()).expect("a dossier");
    assert_eq!(journal_only.places["p1"].checks, None);
    assert!(journal_only.text.contains("checks failing (as last seen)"), "{}", journal_only.text);
    assert!(journal_only.text.contains("- nothing needs you"), "{}", journal_only.text);
    let pulls = vec![PullLive {
        workspace: "/gone/worktrees/rate-limit".into(),
        number: 212,
        state: "open".into(),
        checks: Some("failing".into()),
        ..PullLive::default()
    }];
    let live = build(&range, &entries, &Live { pulls, ..Live::default() }).expect("a dossier");
    assert!(live.text.contains("- [p1] #212 · checks failing"), "{}", live.text);
    assert_eq!(live.places["p1"].checks.as_deref(), Some("failing"));
}

#[test]
fn a_conversation_without_its_pane_reads_closed_even_without_a_closed_entry() {
    let dir = TempDir::new();
    let (path, _) = conversation(dir.path());
    let entries = vec![
        entry(Event::Seen, "2026-11-03T09:12:00.000Z", "c1", &path),
        entry(Event::Prompt, "2026-11-03T09:12:00.000Z", "c1", &path),
        working("2026-11-03T09:12:01.000Z", "c1", &path),
    ];
    let range = resolve(Choice::Now, &entries, seconds("2026-11-04T09:00:00.000Z"), MADRID).expect("now");
    for range in [range, yesterday(&entries)] {
        let text = build(&range, &entries, &Live::default()).expect("a dossier").text;
        assert!(text.contains("closed, last seen") && text.contains("while working"), "{text}");
    }
}

#[test]
fn nothing_needs_you_says_so_in_now_and_yesterday_has_no_list() {
    let dir = TempDir::new();
    let (_, working_entry) = one_conversation(dir.path(), "w");
    let live = Live { panes: vec![live_pane(&working_entry, "working")], ..Live::default() };
    let text = build(&now_range(), &[], &live).expect("a dossier").text;
    assert!(text.contains("## Needs you now\n- nothing needs you\n"), "{text}");
    let (_, entries) = conversation(dir.path());
    assert!(
        !build(&yesterday(&entries), &entries, &Live::default()).expect("a dossier").text.contains("Needs you now")
    );
}

#[test]
fn a_conversation_that_needs_you_is_never_left_out() {
    let dir = TempDir::new();
    let screen: Vec<String> = (0..15).map(|_| "q".repeat(160)).collect();
    let mut panes = Vec::new();
    for n in 0..60 {
        let big = "z".repeat(1500);
        let prompts: Vec<(&str, String, String)> = (0..4)
            .map(|i| ("prompt", format!("2026-11-04T08:0{i}:00.000Z"), format!("{n}-{i} {big}")))
            .chain([("reply", "2026-11-04T08:09:00.000Z".to_string(), big.clone())])
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = prompts.iter().map(|(r, a, t)| (*r, a.as_str(), t.as_str())).collect();
        let path = transcript(dir.path(), &format!("n{n}"), &borrowed);
        let template = entry(Event::Seen, "2026-11-04T08:00:00.000Z", &format!("n{n}"), &path);
        panes.push(LivePane { screen: screen.clone(), ..live_pane(&template, "waiting") });
    }
    let dossier = build(&now_range(), &[], &Live { panes, ..Live::default() }).expect("a dossier");
    assert_eq!(dossier.conversations, 60);
    for n in 1..=60 {
        assert!(dossier.text.contains(&format!("## [c{n}] ")), "c{n}");
        assert!(dossier.text.contains(&format!("- [c{n}] waits for an answer")), "c{n}");
    }
}
