use serde_json::{Value, json};

use super::*;
use crate::control::contract::unknown_fields;
use crate::control::worktrees::{BuildFolder, FailedFolder};
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn answer() -> MergedBuildsFreed {
    let freed = WorktreeFreed {
        outcome: FREED.into(),
        why: None,
        freed: BuildsFreed {
            path: "/wt/shop/issue-106".into(),
            bytes: 27_380_416_512,
            folders: vec![BuildFolder { path: "target".into(), bytes: 27_380_416_512 }],
            ..BuildsFreed::default()
        },
    };
    let refused = WorktreeFreed {
        outcome: REFUSED.into(),
        why: Some("cargo runs in it".into()),
        freed: BuildsFreed {
            path: "/wt/shop/issue-156".into(),
            running: vec!["cargo".into()],
            ..BuildsFreed::default()
        },
    };
    let failed = WorktreeFreed {
        outcome: FAILED.into(),
        why: Some("/wt/shop/issue-164: No such file or directory (os error 2)".into()),
        freed: BuildsFreed { path: "/wt/shop/issue-164".into(), ..BuildsFreed::default() },
    };
    MergedBuildsFreed { check: false, bytes: 27_380_416_512, worktrees: vec![freed, refused, failed] }
}

#[test]
fn the_answer_is_pinned_and_reads_back() {
    let fixture = fixture!("control/free-merged-builds/v1/answer.json");

    assert_eq!(serde_json::to_string_pretty(&answer()).expect("json"), fixture.trim_end());
    assert_eq!(serde_json::from_str::<MergedBuildsFreed>(fixture).expect("the answer"), answer());
}

#[test]
fn each_entry_has_the_keys_of_a_free_builds_answer_and_its_outcome() {
    let entry = serde_json::to_value(&answer().worktrees[0]).expect("json");
    let plain = serde_json::to_value(&answer().worktrees[0].freed).expect("json");

    let mut keys: Vec<&String> = entry.as_object().expect("object").keys().collect();
    keys.retain(|key| *key != "outcome" && *key != "why");
    let mut plain_keys: Vec<&String> = plain.as_object().expect("object").keys().collect();
    keys.sort();
    plain_keys.sort();
    assert_eq!(keys, plain_keys);
}

#[test]
fn an_answer_from_a_newer_server_reads_leniently() {
    let read: MergedBuildsFreed = serde_json::from_value(
        json!({"check": true, "worktrees": [{"outcome": "new", "path": "/x", "more": 1}], "also": 2}),
    )
    .expect("the answer");

    assert!(read.check);
    assert_eq!(read.worktrees[0].outcome, "new");
    assert_eq!(read.worktrees[0].freed.path.to_str(), Some("/x"));
}

#[test]
fn failed_folders_are_part_of_an_entry() {
    let entry = WorktreeFreed {
        outcome: FREED.into(),
        freed: BuildsFreed {
            failed: vec![FailedFolder { path: "target".into(), error: "busy".into() }],
            ..BuildsFreed::default()
        },
        ..WorktreeFreed::default()
    };

    let json = serde_json::to_value(&entry).expect("json");

    assert_eq!(json["failed"][0]["error"], "busy");
}

#[test]
fn the_documented_request_reads_with_no_stray_field_and_writes_every_field() {
    let raw: Value = serde_json::from_str(fixture!("control/free-merged-builds/v1/request.json")).expect("the request");

    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");

    assert_eq!(unknown_fields(&raw, &parsed), None);
    assert_eq!(parsed.command, Command::FreeMergedBuilds(FreeMergedBuilds { project: Some(2), check: true }));
    let written = Command::FreeMergedBuilds(FreeMergedBuilds::default());
    assert_eq!(serde_json::to_value(&written).expect("json")["args"], json!({"project": null, "check": false}));
}

#[test]
fn the_log_holds_the_project_and_the_check() {
    let request = FreeMergedBuilds { project: Some(2), check: true };

    assert_eq!(
        Command::FreeMergedBuilds(request).fields(),
        [("project", "2".to_string()), ("check", "true".to_string())]
    );
}
