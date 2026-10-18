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
