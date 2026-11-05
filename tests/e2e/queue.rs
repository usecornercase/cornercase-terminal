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

fn repository_project(app: &Harness, name: &str) -> String {
    let repo = temp_dir(name);
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join("README"), "hello\n").expect("write a file");
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    app.session.says(&["open", &repo.display().to_string(), "--focus"])
}

fn entries(app: &Harness) -> Value {
    serde_json::from_str(&app.session.says(&["queue", "list", "--json"])).expect("queue JSON")
}

#[test]
fn a_routine_is_added_listed_paused_resumed_and_survives_a_restart() {
    let mut app = Harness::start();
    let project = repository_project(&app, "routine-shop");
    let id = app.session.says(&[
        "queue",
        "add",
        "--every",
        "2h",
        "--project",
        &project,
        "--agent",
        "claude",
        "--prompt",
        "review the pull requests",
    ]);
    let routine = entries(&app)["entries"][0].clone();
    assert_eq!(routine["id"].as_u64().expect("id").to_string(), id);
    assert_eq!((&routine["repeat"], &routine["state"]), (&"2h".into(), &"pending".into()));
    assert_eq!(routine["next"], routine["at"]);
    assert!(app.session.says(&["queue", "list"]).contains(" · ↻ every 2h"));
    app.session.says(&["queue", "pause", &id]);
    let paused = entries(&app)["entries"][0].clone();
    assert!(paused.get("next").is_none());
    assert_eq!(paused["paused"], true);
    assert!(app.session.says(&["queue", "list"]).contains(" paused "));
    app.session.says(&["queue", "resume", &id]);
    assert!(entries(&app)["entries"][0].get("next").is_some());
    app.restart_server();
    let restored = entries(&app)["entries"][0].clone();
    assert_eq!((restored["id"].as_u64().map(|id| id.to_string()), &restored["repeat"]), (Some(id), &"2h".into()));
}

#[test]
fn a_routine_with_a_gap_that_is_too_short_or_a_plain_folder_is_refused() {
    let app = Harness::start();
    let project = repository_project(&app, "routine-gap");
    let short = app
        .session
        .command()
        .args(["queue", "add", "--every", "10m", "--project", &project, "--agent", "claude", "--prompt", "go"])
        .output()
        .expect("refused");
    assert_eq!(short.status.code(), Some(1), "{short:?}");
    assert!(String::from_utf8_lossy(&short.stderr).contains("30 minutes"), "{short:?}");
    let mixed = app
        .session
        .command()
        .args(["queue", "add", "--every", "2h", "--issue", "7", "--agent", "claude"])
        .output()
        .expect("wrong usage");
    assert_eq!(mixed.status.code(), Some(2), "{mixed:?}");
    let folder = temp_dir("routine-folder");
    let plain = app.session.says(&["open", &folder.display().to_string(), "--focus"]);
    let refused = app
        .session
        .command()
        .args(["queue", "add", "--every", "2h", "--project", &plain, "--agent", "claude", "--prompt", "go"])
        .output()
        .expect("refused");
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("plain folder"), "{refused:?}");
    assert_eq!(entries(&app)["entries"], serde_json::json!([]));
}

#[test]
fn removing_a_routine_prints_an_undo_and_restore_puts_it_back() {
    let app = Harness::start();
    let project = repository_project(&app, "routine-undo");
    let id = app.session.says(&[
        "queue",
        "add",
        "--every",
        "day 09:00",
        "--project",
        &project,
        "--agent",
        "claude",
        "--prompt",
        "triage",
    ]);
    let before = entries(&app);
    let undo = app.session.says(&["queue", "remove", &id]);
    assert_eq!(entries(&app)["entries"], serde_json::json!([]));
    app.session.says(&["queue", "restore", &undo]);
    assert_eq!(entries(&app), before);
}
