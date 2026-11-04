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
    assert!(text.find("request number 9").expect("nine") < text.find("request number 4").expect("four"));
    assert!(!text.contains("request number 3"), "{text}");
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
    assert!(text.contains(&format!("\"{}…\"", "x".repeat(500))), "{text}");
    assert!(text.contains("trace (pasted 41 lines)"));
}

#[test]
fn past_the_total_cap_later_conversations_keep_only_their_header() {
    let dir = TempDir::new();
    let mut entries = Vec::new();
    for n in 0..40 {
        let prompts: Vec<(&str, String, String)> = (0..8)
            .map(|i| ("prompt", format!("2026-11-03T09:{i:02}:00.000Z"), format!("{n}-{i} {}", "y".repeat(480))))
            .collect();
        let borrowed: Vec<(&str, &str, &str)> = prompts.iter().map(|(r, a, t)| (*r, a.as_str(), t.as_str())).collect();
        let path = transcript(dir.path(), &format!("c{n}"), &borrowed);
        entries.push(working("2026-11-03T09:00:00.000Z", &format!("c{n}"), &path));
    }
    let range = yesterday(&entries);
    let text = build(&range, &entries, &Live::default()).expect("a dossier").text;
    assert!(text.len() < 61 * 1024, "{}", text.len());
    assert!(text.contains("conversations were shortened"), "{}", &text[..300]);
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
