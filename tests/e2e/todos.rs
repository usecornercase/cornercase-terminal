use super::commands::Followed;
use super::*;

#[test]
fn the_todo_list_is_edited_reordered_removed_and_restored_by_commands() {
    let app = Harness::start();
    let a = app.session.says(&["todo", "add", "first"]);
    let b = app.session.says(&["todo", "add", "second"]);
    let c = app.session.says(&["todo", "add", "third"]);

    app.session.says(&["todo", "edit", &c, "third", "one"]);
    app.session.says(&["todo", "done", &a]);
    app.session.says(&["todo", "undone", &a]);
    app.session.says(&["todo", "move", &c, "--before", &a]);
    let order = app.session.says(&["todo", "list"]);
    let undo = app.session.says(&["todo", "rm", &b]);
    let after_rm = app.session.says(&["todo", "list"]);
    app.session.says(&["todo", "restore", &undo]);
    app.session.says(&["todo", "done", &a]);
    let cleared = app.session.says(&["todo", "clear-done"]);
    let kept = app.session.says(&["todo", "list"]);
    app.session.says(&["todo", "restore", &cleared]);
    let restored = app.session.says(&["todo", "list"]);

    assert_eq!(order, format!("{b} [ ] second\n{c} [ ] third one\n{a} [ ] first"));
    assert_eq!(after_rm, format!("{c} [ ] third one\n{a} [ ] first"));
    assert_eq!(kept, format!("{b} [ ] second\n{c} [ ] third one"));
    assert_eq!(restored, format!("{b} [ ] second\n{c} [ ] third one\n{a} [x] first"));
}

#[test]
fn the_app_state_is_printed_and_followed() {
    let app = Harness::start();
    let first = app.session.says(&["todo", "add", "first"]);

    let printed: serde_json::Value =
        serde_json::from_str(&app.session.says(&["status", "--app"])).expect("one JSON document");
    let mut following = app.session.spawn(&["status", "--app", "--follow"]);
    let mut followed = Followed::of(&mut following);
    followed.until("the first line", |line| line["todos"][0]["text"] == "first");
    let revision = followed.seen[0]["revision"].as_u64().expect("a revision");
    let second = app.session.says(&["todo", "add", "second"]);
    followed.until("the next line", |line| line["todos"].as_array().is_some_and(|todos| todos.len() == 2));

    assert_eq!(printed["todos"][0]["id"].to_string(), first);
    assert_eq!(printed["boot"], followed.seen[0]["boot"]);
    assert_eq!(followed.seen[0]["away"].as_array().map(Vec::len), Some(2));
    assert!(followed.seen.last().expect("a line")["revision"].as_u64() > Some(revision));
    assert_eq!(followed.seen.last().expect("a line")["todos"][1]["id"].to_string(), second);
    app.session.says(&["kill-server"]);
    let status = following.wait().expect("the command ends");
    assert!(status.success(), "{status:?}");
}
