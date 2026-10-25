use serde_json::Value;

use super::*;

#[test]
fn queue_commands_add_list_edit_and_remove_without_starting_an_agent() {
    let app = Harness::start();
    let before = app.session.report();
    let id = app.session.says(&["queue", "add", "--at", "in 2h", "--agent", "claude", "--prompt", "fix the login"]);
    let list: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("queue JSON");
    let entry = &list["entries"][0];
    assert_eq!(entry["id"].as_u64().expect("id").to_string(), id);
    assert_eq!((&entry["state"], &entry["prompt"]), (&"pending".into(), &"fix the login".into()));
    assert!(app.session.says(&["queue", "list"]).contains("fix the login"));
    app.session.says(&["queue", "edit", &id, "--at", "in 3h", "--prompt", "fix logout"]);
    let edited: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("edited JSON");
    assert_eq!(edited["entries"][0]["prompt"], "fix logout");
    assert!(edited["entries"][0]["at"].as_i64().expect("new time") > entry["at"].as_i64().expect("old time"));
    app.session.says(&["queue", "edit", &id, "--name", "logout fix"]);
    let renamed: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("renamed JSON");
    let (renamed, edited) = (&renamed["entries"][0], &edited["entries"][0]);
    assert_eq!(
        (&renamed["label"], &renamed["at"], &renamed["prompt"]),
        (&"logout fix".into(), &edited["at"], &edited["prompt"])
    );
    app.session.says(&["queue", "remove", &id]);
    let list: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("empty JSON");
    assert_eq!(list["entries"], serde_json::json!([]));
    assert_eq!(app.session.report().projects[0].workspaces[0].tabs.len(), before.projects[0].workspaces[0].tabs.len());
}

#[test]
fn a_pending_entry_and_its_id_survive_a_server_restart() {
    let mut app = Harness::start();
    let id = app.session.says(&[
        "queue",
        "add",
        "--at",
        "in 2h",
        "--agent",
        "claude",
        "--prompt",
        "follow up after restart",
    ]);
    app.restart_server();
    let list: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("restored queue");
    assert_eq!(list["entries"][0]["id"].as_u64().expect("id").to_string(), id);
    assert_eq!(list["entries"][0]["state"], "pending");
    assert_eq!(list["entries"][0]["prompt"], "follow up after restart");
}

#[test]
fn queue_removals_print_undo_numbers_and_restore_the_saved_entry() {
    let mut app = Harness::start();
    let id = app.session.says(&["queue", "add", "--at", "in 2h", "--agent", "claude", "--prompt", "review login"]);
    let before: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("entries");
    let removed: Value = serde_json::from_str(&app.session.says(&["queue", "remove", &id, "--json"])).expect("removal");
    assert_eq!(removed["entries"], before["entries"]);
    let undo = removed["undo"].as_u64().expect("undo number");
    assert_eq!(app.session.says(&["queue", "restore", &undo.to_string()]), "");

    let next = app.session.says(&["queue", "remove", &id]);
    assert_ne!(next, undo.to_string());
    app.session.says(&["queue", "restore", &next]);
    app.restart_server();
    let restored: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("restored");
    assert_eq!(restored, before);
}

#[test]
fn clearing_an_empty_started_history_prints_no_undo() {
    let app = Harness::start();
    assert_eq!(app.session.says(&["queue", "clear-started"]), "");
    let empty: Value = serde_json::from_str(&app.session.says(&["queue", "clear-started", "--json"])).expect("empty");
    assert_eq!(empty, serde_json::json!({"entries": []}));
    let refused = app.session.command().args(["queue", "restore", "999"]).output().expect("restore refused");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("no queue removal 999"));
}

#[test]
fn an_expired_undo_number_cannot_restore_a_removal_from_the_next_server() {
    let mut app = Harness::start();
    let args = ["queue", "add", "--at", "in 2h", "--agent", "claude", "--prompt", "review login"];
    let id = app.session.says(&args);
    let expired = app.session.says(&["queue", "remove", &id]);
    app.restart_server();
    let next = app.session.says(&args);
    let current = app.session.says(&["queue", "remove", &next]);
    assert_ne!(current, expired);
    let refused = app.session.command().args(["queue", "restore", &expired]).output().expect("old undo refused");
    assert!(!refused.status.success());
    let empty: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("still removed");
    assert_eq!(empty, serde_json::json!({"entries": []}));
    app.session.says(&["queue", "restore", &current]);
    let restored: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("restored");
    assert_eq!(restored["entries"][0]["id"].as_u64().expect("new entry").to_string(), next);
}
