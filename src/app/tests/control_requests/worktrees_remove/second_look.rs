use std::time::Duration;

use super::*;
use crate::app::worktrees::{RemovalStep, WorktreeEvent};

fn next_step(s: &mut Closed, listed: bool) -> AppEvent {
    loop {
        let event = s.rx.recv_timeout(Duration::from_secs(30)).expect("an event");
        match &event {
            AppEvent::Worktree(WorktreeEvent::Removal { step, .. })
                if listed == matches!(**step, RemovalStep::Listed(_)) =>
            {
                return event;
            }
            _ => s.app.handle_event(event, AREA).expect("handle event"),
        }
    }
}

fn step(s: &mut Closed, event: AppEvent) {
    s.app.handle_event(event, AREA).expect("handle event");
}

fn until_the_look_ended(s: &mut Closed, args: &Value) -> AppEvent {
    request_json(&mut s.app, COMMAND, args);
    let listed = next_step(s, true);
    step(s, listed);
    next_step(s, false)
}

pub(super) fn after_the_look(s: &mut Closed, args: &Value, change: impl FnOnce()) -> Value {
    let looked = until_the_look_ended(s, args);
    change();
    step(s, looked);
    reply_later(&mut s.app, &s.rx).expect("an answer")["folders"][0].clone()
}

#[test]
fn a_branch_switched_after_the_look_is_kept_and_named() {
    let (mut s, at) = with_one("a");
    let checkout = at.clone();

    let folder = after_the_look(&mut s, &json!({"paths": [at], "delete_branch": true}), move || {
        git(&checkout, &["checkout", "--quiet", "-b", "other"]);
    });

    assert_eq!((outcome(&folder), &folder["branches"]), ("removed", &json!([])));
    assert_eq!(folder["kept"], json!(["kept the branch a: it changed since the confirmation"]));
    assert!(s.has_branch("a") && s.has_branch("other"));
}

#[test]
fn a_file_written_after_the_look_stops_git_without_force_and_the_folder_stays() {
    let (mut s, at) = with_one("a");
    let late = at.clone();

    let folder = after_the_look(&mut s, &json!({"paths": [at]}), move || write_notes(&late));

    assert_eq!(outcome(&folder), "failed");
    assert!(why(&folder).contains("use --force"), "{folder}");
    assert!(at.join("notes.txt").exists());
}

#[test]
fn a_leftover_whose_repository_came_back_after_the_look_is_not_deleted() {
    let mut s = closed_repo();
    let admin = TempDir::new();
    let gitdir = admin.path().join("back");
    let at = s.leftover_folder(&gitdir.display().to_string());

    let folder = after_the_look(&mut s, &json!({"paths": [at], "force": true}), move || {
        std::fs::create_dir(&gitdir).expect("mkdir");
    });

    assert_eq!(outcome(&folder), "failed");
    assert!(why(&folder).contains("is back"), "{folder}");
    assert!(at.join("notes.txt").exists());
}

#[test]
fn a_client_gone_before_the_look_ended_removes_nothing() {
    let (mut s, at) = with_one("a");
    let looked = until_the_look_ended(&mut s, &json!({"paths": [at], "delete_branch": true}));

    s.app.forget(CLIENT);
    step(&mut s, looked);
    drain(&mut s.app, &s.rx);

    assert!(at.join("README").exists() && s.has_branch("a"));
    assert!(s.app.git.cleanup.pending.is_empty());
    assert_eq!(s.app.take_answers(), Vec::new());
}

#[test]
fn a_client_gone_before_the_listing_ended_starts_no_look() {
    let (mut s, at) = with_one("a");
    request_json(&mut s.app, COMMAND, &json!({"paths": [at]}));

    s.app.forget(CLIENT);
    let listed = next_step(&mut s, true);
    step(&mut s, listed);

    assert!(at.join("README").exists() && s.app.git.cleanup.pending.is_empty());
    assert_eq!(s.app.take_answers(), Vec::new());
}
