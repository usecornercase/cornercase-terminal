use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};
use crate::control::worktrees::FailedFolder;
use crate::text::MB;

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
        "freed 200 MB in /srv/x: node_modules 200 MB\nrunning: node\ncould not free target: busy\nkept dist: no longer ignored by git or gone\n"
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
