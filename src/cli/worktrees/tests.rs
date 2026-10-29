use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};
use crate::control::worktrees::FailedFolder;
use crate::text::MB;
use crate::time::Stamp;

fn parsed(args: &[&str]) -> std::result::Result<WorktreesAction, clap::Error> {
    let mut all = vec!["cornercase", "worktrees"];
    all.extend(args);
    match Cli::try_parse_from(all)?.command {
        Some(Command::Control(Control::Worktrees(args))) => Ok(args.action),
        other => panic!("not a worktrees command: {other:?}"),
    }
}

fn sized() -> WorktreeSized {
    WorktreeSized {
        path: "/home/ana/.cornercase/worktrees/shop/feat-x".into(),
        bytes: 300 * MB,
        builds_bytes: 200 * MB,
        builds: vec![BuildFolder { path: "node_modules".into(), bytes: 200 * MB }],
        nested: vec![".venv/src/foo/.git".into()],
    }
}

#[test]
fn size_says_the_total_the_builds_and_each_folder() {
    assert_eq!(
        size_text(&sized(), Some(Path::new("/home/ana"))),
        "300 MB  ~/.cornercase/worktrees/shop/feat-x  (200 MB of builds)\n  200 MB  node_modules\n  a git checkout inside a build folder: .venv/src/foo/.git\n"
    );
}

#[test]
fn free_says_what_went_what_failed_and_what_runs() {
    let freed = BuildsFreed {
        path: "/srv/x".into(),
        bytes: 200 * MB,
        folders: vec![BuildFolder { path: "node_modules".into(), bytes: 200 * MB }],
        kept: vec!["dist".into()],
        failed: vec![FailedFolder { path: "target".into(), error: "busy".into() }],
        running: vec!["node".into()],
        ..BuildsFreed::default()
    };
    assert_eq!(
        free_text(&freed, None),
        "freed 200 MB in /srv/x: node_modules 200 MB\nrunning: node\ncould not free target: busy\nkept dist: not freed: no longer an ignored folder here, or on another filesystem\n"
    );
    let check = BuildsFreed { check: true, failed: Vec::new(), kept: Vec::new(), ..freed };
    assert!(free_text(&check, None).starts_with("would free 200 MB in /srv/x"));
}

#[test]
fn nothing_to_free_is_said_so() {
    let none = BuildsFreed { path: "/srv/x".into(), ..BuildsFreed::default() };
    assert_eq!(free_text(&none, None), "no build folders to free in /srv/x\n");
}

#[test]
fn a_target_takes_one_way_to_name_the_worktree() {
    assert!(parsed(&["size", "--workspace", "3", "--path", "/x"]).is_err());
    assert!(parsed(&["free-builds", "--worktree", "feat/x", "--workspace", "3"]).is_err());
    assert!(matches!(
        parsed(&["free-builds", "--path", "/x", "--check", "--force"]),
        Ok(WorktreesAction::FreeBuilds(_))
    ));
    assert!(parsed(&[]).is_err());
}

fn listing() -> WorktreeListing {
    let sized = |bytes: u64, builds: u64| WorktreeSized { bytes, builds_bytes: builds, ..WorktreeSized::default() };
    let merged = WorktreeInfo {
        path: "/home/ana/.cornercase/worktrees/shop/fix-login".into(),
        kind: "linked".into(),
        project_name: "shop".into(),
        branch: Some("fix/login".into()),
        workspace: Some(7),
        last_changed: Some("2026-10-01T00:00:00.000Z".into()),
        merged: true,
        safe: true,
        size: Some(sized(2400 * MB, 2100 * MB)),
        ..WorktreeInfo::default()
    };
    let busy = WorktreeInfo {
        path: "/home/ana/.cornercase/worktrees/shop/feat-gift".into(),
        branch: Some("feat/gift".into()),
        last_changed: Some("2026-10-29T10:00:00.000Z".into()),
        merged: false,
        risks: vec!["3 files changed".into(), "codex running in it; they stop".into()],
        size: Some(sized(1100 * MB, 900 * MB)),
        ..merged.clone()
    };
    let orphan = WorktreeInfo {
        path: "/home/ana/.cornercase/worktrees/api/side".into(),
        kind: "repository-gone".into(),
        project_name: "api".into(),
        branch: None,
        workspace: None,
        last_changed: None,
        merged: false,
        risks: vec!["its repository is gone; deletes the folder and its files".into()],
        size: Some(sized(12 * MB, 0)),
        ..merged.clone()
    };
    WorktreeListing { worktrees: vec![orphan, merged, busy] }
}

#[test]
fn list_says_each_worktree_its_reasons_and_the_total() {
    let now = Stamp::parse("2026-10-29T12:00:00.000Z").expect("a time");

    let text = list_text(&listing(), Some(Path::new("/home/ana")), now);

    assert_eq!(
        text,
        "api › side  ~/.cornercase/worktrees/api/side  12 MB (0 KB of builds)  repository gone\n  its repository is gone; deletes the folder and its files\n\
shop › fix/login  ~/.cornercase/worktrees/shop/fix-login  28 days ago  2.3 GB (2.1 GB of builds)  merged  workspace 7\n  nothing uncommitted or unpushed\n\
shop › feat/gift  ~/.cornercase/worktrees/shop/feat-gift  2h ago  1.1 GB (900 MB of builds)  workspace 7\n  3 files changed\n  codex running in it; they stop\n\
3.4 GB in 3 worktrees · 2.1 GB of builds idle 7+ days\n"
    );
}

#[test]
fn an_empty_list_says_so() {
    assert_eq!(list_text(&WorktreeListing::default(), None, SystemTime::now()), "no worktrees\n");
}

#[test]
fn a_row_not_checked_or_not_measured_is_marked_and_not_counted() {
    let now = Stamp::parse("2026-10-29T12:00:00.000Z").expect("a time");
    let failed = WorktreeInfo { size: None, size_error: Some("gave up".into()), ..listing().worktrees[1].clone() };
    let text = list_text(&WorktreeListing { worktrees: vec![failed, listing().worktrees[2].clone()] }, None, now);
    assert!(text.contains("  ?  merged"), "{text}");
    assert!(text.ends_with("1.1 GB+ in 1 worktree\n"), "{text}");
}

#[test]
fn list_takes_a_project_and_no_sizes_and_nothing_else() {
    let list = parsed(&["list", "--project", "3", "--no-sizes", "--json"]).expect("parses");
    assert!(matches!(list, WorktreesAction::List(ListArgs { project: Some(3), no_sizes: true, .. })));
    assert!(parsed(&["list", "--project", "x"]).is_err());
    assert!(parsed(&["list", "--path", "/x"]).is_err());
}
