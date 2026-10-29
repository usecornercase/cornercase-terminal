use std::time::{Duration, SystemTime};

use serde_json::json;

use super::*;
use crate::git::worktree::{Holder, Lock};
use crate::worktrees::builds::{Folder, Size};
use crate::worktrees::{Found, Live, Scan};

fn found(kind: Kind) -> Found {
    Found {
        repo: "/code/shop".into(),
        project: "shop".into(),
        path: "/wt/shop/feat-x".into(),
        branch: Some("feat/x".into()),
        admin: None,
        kind,
    }
}

fn row(kind: Kind) -> Row {
    Row {
        found: found(kind),
        live: None,
        scan: None,
        size: None,
        outside: Vec::new(),
        elsewhere: Vec::new(),
        ticked: false,
        removing: false,
        freeing: false,
        gone: false,
    }
}

fn scan() -> Scan {
    Scan {
        last_changed: Some(SystemTime::UNIX_EPOCH + Duration::from_hours(24)),
        changed: 3,
        unpushed: 2,
        upstream_gone: false,
        merged: false,
        has_upstream: true,
        branch: Some("feat/x".into()),
        lock: Some(Lock { reason: "ci".into(), holder: Some(Holder::Gone(7)), role: None }),
    }
}

fn live() -> Live {
    Live {
        project: 2,
        workspace: 9,
        running: vec!["codex".into()],
        removing: false,
        role_of_run: false,
        run_roles: 0,
        pull_merged: false,
    }
}

#[test]
fn a_list_request_measures_unless_told_not_to() {
    let default: ListWorktrees = serde_json::from_value(json!({})).expect("parse");
    let skipped: ListWorktrees = serde_json::from_value(json!({"sizes": false, "project": 4})).expect("parse");
    assert_eq!(default, ListWorktrees { project: None, sizes: true });
    assert_eq!(skipped, ListWorktrees { project: Some(4), sizes: false });
}

#[test]
fn a_row_carries_its_facts_its_verdict_and_its_size() {
    let size =
        Size { bytes: 900, builds: vec![Folder { path: "node_modules".into(), bytes: 400 }], nested: Vec::new() };
    let checked = Row { scan: Some(Ok(scan())), live: Some(live()), size: Some(Ok(size)), ..row(Kind::Linked) };

    let info = WorktreeInfo::of(&checked, vec!["3 files changed".into()]);

    assert_eq!((info.kind.as_str(), info.safe, info.changed, info.unpushed), ("linked", false, Some(3), Some(2)));
    assert_eq!((info.project, info.workspace), (Some(2), Some(9)));
    assert_eq!(info.last_changed.as_deref(), Some("1970-01-02T00:00:00.000Z"));
    assert_eq!(info.repository, Some("/code/shop".into()));
    assert_eq!(info.lock.map(|lock| (lock.running, lock.pid)), Some((Some(false), Some(7))));
    let sized = info.size.expect("sized");
    assert_eq!((sized.bytes, sized.builds_bytes, sized.builds[0].path.as_str()), (900, 400, "node_modules"));
}

#[test]
fn a_clean_row_is_safe_and_a_failed_scan_is_not() {
    let clean = Scan { changed: 0, lock: None, ..scan() };
    let ok = WorktreeInfo::of(&Row { scan: Some(Ok(clean)), ..row(Kind::Linked) }, Vec::new());
    assert!(ok.safe && ok.risks.is_empty());

    let failed = WorktreeInfo::of(&Row { scan: Some(Err("gave up".into())), ..row(Kind::Linked) }, Vec::new());
    assert_eq!((failed.safe, failed.error.as_deref(), failed.changed), (false, Some("gave up"), None));
}

#[test]
fn an_orphan_has_no_repository_and_a_prunable_entry_counts_commits_only() {
    let orphan = WorktreeInfo::of(&row(Kind::Orphan), Vec::new());
    assert_eq!((orphan.kind.as_str(), orphan.repository, orphan.safe), ("repository-gone", None, false));

    let prunable = Row { scan: Some(Ok(Scan { changed: 0, lock: None, ..scan() })), ..row(Kind::Prunable) };
    let info = WorktreeInfo::of(&prunable, Vec::new());
    assert_eq!(
        (info.kind.as_str(), info.changed, info.unpushed, info.size.is_none()),
        ("prunable", None, Some(2), true)
    );
}
