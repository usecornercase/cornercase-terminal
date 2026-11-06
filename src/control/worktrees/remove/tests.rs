use serde_json::{Value, json};

use super::*;
use crate::control::arrange::{Locked, RoleTree};
use crate::control::contract::unknown_fields;
use crate::control::{Command, Request};
use crate::test_util::fixture;

fn answer() -> WorktreesRemoved {
    let worktree = WorktreeInfo {
        path: "/wt/shop/feat-old".into(),
        kind: "linked".into(),
        project_name: "shop".into(),
        repository: Some("/code/shop".into()),
        branch: Some("feat/old".into()),
        changed: Some(1),
        unpushed: Some(2),
        lock: Some(Locked { reason: "ci".into(), path: "/wt/shop/feat-old".into(), ..Locked::default() }),
        risks: vec!["1 file changed".into(), "2 commits not pushed".into()],
        ..WorktreeInfo::default()
    };
    let refused = FolderRemoved {
        path: "/wt/shop/feat-old".into(),
        outcome: REFUSED.into(),
        why: Some("1 file changed; add --force to remove it anyway".into()),
        needs: vec![FORCE.into(), UNLOCK.into()],
        lines: vec!["1 file changed".into(), "2 commits not pushed".into(), "teardown runs first".into()],
        lost: vec!["feat/old: its commits will be lost".into()],
        teardown: true,
        roles: vec![RoleTree { role: "coder".into(), path: "/wt/shop/coder".into(), branch: "coder".into() }],
        worktree: Some(worktree),
        ..FolderRemoved::default()
    };
    let removed = FolderRemoved {
        path: "/wt/shop/lead".into(),
        outcome: REMOVED.into(),
        removed_with: Some("/wt/shop/run".into()),
        branches: vec!["lead".into()],
        kept: vec!["kept the branch x: it changed since the confirmation".into()],
        warnings: vec!["lead: the teardown failed".into()],
        ..FolderRemoved::default()
    };
    let failed = FolderRemoved {
        path: "/wt/shop/stuck".into(),
        outcome: FAILED.into(),
        why: Some("contains modified or untracked files, use --force to delete it".into()),
        ..FolderRemoved::default()
    };
    WorktreesRemoved { check: false, folders: vec![refused, removed, failed] }
}

#[test]
fn the_answer_is_pinned_and_reads_back() {
    let fixture = fixture!("control/worktrees-remove/v1/answer.json");

    assert_eq!(serde_json::to_string_pretty(&answer()).expect("json"), fixture.trim_end());
    assert_eq!(serde_json::from_str::<WorktreesRemoved>(fixture).expect("the answer"), answer());
}

#[test]
fn an_answer_from_a_newer_server_reads_leniently() {
    let read: WorktreesRemoved = serde_json::from_value(
        json!({"check": true, "folders": [{"path": "/x", "outcome": "new", "more": 1}], "also": 2}),
    )
    .expect("the answer");

    assert!(read.check);
    assert_eq!(read.folders[0].outcome, "new");
}

#[test]
fn the_documented_request_reads_with_no_stray_field_and_writes_every_field() {
    let raw: Value = serde_json::from_str(fixture!("control/worktrees-remove/v1/request.json")).expect("the request");

    let parsed: Request = serde_json::from_value(raw.clone()).expect("a request");

    assert_eq!(unknown_fields(&raw, &parsed), None);
    assert_eq!(
        parsed.command,
        Command::WorktreesRemove(WorktreesRemove {
            paths: vec!["/wt/shop/feat-old".into()],
            check: true,
            force: true,
            unlock: true,
            delete_branch: true,
        })
    );
    let written = Command::WorktreesRemove(WorktreesRemove::default());
    assert_eq!(
        serde_json::to_value(&written).expect("json")["args"],
        json!({"paths": [], "check": false, "force": false, "unlock": false, "delete_branch": false})
    );
}

#[test]
fn the_log_holds_the_folders_and_the_flags() {
    let request = WorktreesRemove { paths: vec!["/a".into(), "/b".into()], check: true, ..WorktreesRemove::default() };

    assert_eq!(
        Command::WorktreesRemove(request).fields(),
        [
            ("paths", "/a,/b".to_string()),
            ("check", "true".to_string()),
            ("force", "false".to_string()),
            ("unlock", "false".to_string()),
            ("delete_branch", "false".to_string()),
        ]
    );
}
