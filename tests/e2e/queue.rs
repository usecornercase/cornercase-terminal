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
    assert_eq!(entry["state"], "pending");
    assert_eq!(entry["prompt"], "fix the login");
    assert!(app.session.says(&["queue", "list"]).contains("fix the login"));
    app.session.says(&["queue", "edit", &id, "--at", "in 3h", "--prompt", "fix logout"]);
    let edited: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("edited JSON");
    assert_eq!(edited["entries"][0]["prompt"], "fix logout");
    assert!(edited["entries"][0]["at"].as_i64().expect("new time") > entry["at"].as_i64().expect("old time"));
    app.session.says(&["queue", "edit", &id, "--name", "logout fix"]);
    let renamed: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("renamed JSON");
    assert_eq!(renamed["entries"][0]["label"], "logout fix");
    assert_eq!(renamed["entries"][0]["at"], edited["entries"][0]["at"]);
    assert_eq!(renamed["entries"][0]["prompt"], edited["entries"][0]["prompt"]);
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
    app.send(b"echo before-\"\"restart\r");
    app.wait_for("the old shell answers", |text| text.contains("before-restart"));
    app.session.says(&["restart", "--yes"]);
    app.wait_for("the new shell comes back", |text| !text.contains("before-restart") && text.contains(&first_entry()));
    let list: Value = serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("restored queue");
    assert_eq!(list["entries"][0]["id"].as_u64().expect("id").to_string(), id);
    assert_eq!(list["entries"][0]["state"], "pending");
    assert_eq!(list["entries"][0]["prompt"], "follow up after restart");
}
