use super::routines::{entry, listed, shop};
use super::*;

fn one_off(app: &mut App, extra: &Value) -> u64 {
    let project = app.projects[0].id;
    let mut entry = json!({"zone": "UTC", "prompt": "x", "agent": "claude", "at": "in 3h", "project": project});
    if let (Some(base), Some(more)) = (entry.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    ok(app, "queue", &json!({"action": "add", "entry": entry}))["id"].as_u64().expect("id")
}

fn edited(app: &mut App, id: u64, extra: &Value) {
    let mut add = json!({"zone": "UTC", "at": "in 4h"});
    if let (Some(base), Some(more)) = (add.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    ok(app, "queue", &json!({"action": "edit", "entry": {"id": id, "add": add}}));
}

fn kept(app: &App, id: u64) -> Option<bool> {
    app.panels.queued.queue().entry(id).expect("the entry").task.fresh
}

#[test]
fn an_entry_keeps_what_queue_add_chose() {
    let mut shop = shop();
    let fresh = one_off(&mut shop.app, &json!({"worktree": "feat/x", "fresh": true}));
    let head = one_off(&mut shop.app, &json!({"worktree": "feat/y", "fresh": false}));
    let neither = one_off(&mut shop.app, &json!({"worktree": "feat/z"}));

    assert_eq!(
        [kept(&shop.app, fresh), kept(&shop.app, head), kept(&shop.app, neither)],
        [Some(true), Some(false), None]
    );
    assert_eq!(listed(&mut shop.app, fresh)["fresh"], json!(true));
    assert!(listed(&mut shop.app, neither).get("fresh").is_none());
}

#[test]
fn an_edit_without_the_choice_keeps_it_and_one_with_it_changes_it() {
    let mut shop = shop();
    let id = one_off(&mut shop.app, &json!({"worktree": "feat/x", "fresh": true}));

    edited(&mut shop.app, id, &json!({}));
    assert_eq!(kept(&shop.app, id), Some(true));

    edited(&mut shop.app, id, &json!({"fresh": false}));
    assert_eq!(kept(&shop.app, id), Some(false));
}

#[test]
fn a_routine_refuses_no_fresh() {
    let mut shop = shop();

    let error = refused(&mut shop.app, "queue", &json!({"action": "add", "entry": entry(&json!({"fresh": false}))}));

    assert!(error.message.contains("always start from the up-to-date default branch"), "{}", error.message);
}
