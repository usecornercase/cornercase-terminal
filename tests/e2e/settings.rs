use serde_json::Value;

use super::*;

fn setting(session: &Session, key: &str) -> Value {
    let described: Value = serde_json::from_str(&session.says(&["config", "--json"])).expect("a description");
    let found = described["settings"].as_array().expect("settings").iter().find(|s| s["key"] == key);
    found.expect("a setting")["value"].clone()
}

fn wait_for_setting(session: &Session, key: &str, expected: &Value) {
    let deadline = Instant::now() + TIMEOUT;
    while setting(session, key) != *expected {
        assert!(Instant::now() < deadline, "timed out waiting for {key} to be {expected}");
        thread::sleep(POLL);
    }
}

#[test]
fn settings_are_changed_through_the_server_and_an_edit_of_the_file_is_picked_up() {
    let session = Session::new();
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    assert_eq!(setting(&session, "counts"), Value::Bool(true));

    let said = session.says(&["config", "set", "counts", "false"]);
    assert!(said.contains("no longer show"), "{said}");
    session.wait_for_file("config.json", "the setting is saved", |text| text.contains("\"counts\": false"));
    assert_eq!(setting(&session, "counts"), Value::Bool(false));

    let path = session.dir.join("config.json");
    let text = std::fs::read_to_string(&path).expect("the saved file");
    std::fs::write(&path, text.replace("\"counts\": false", "\"counts\": true")).expect("edit the file");
    wait_for_setting(&session, "counts", &Value::Bool(true));

    let refused = session.cli(&["config", "set", "worktrees_dir", "relative"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("use an absolute path"), "{refused:?}");
}

fn started() -> Arc<Session> {
    let session = Session::new();
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    session
}

fn colleague_with_a_tree() -> Arc<Session> {
    let colleague = started();
    colleague.says(&["config", "set", "sidebar", "tree"]);
    colleague
}

#[test]
fn a_setup_exported_by_one_server_imports_into_another() {
    let colleague = colleague_with_a_tree();
    let file = colleague.dir.join("setup.toml");
    colleague.says(&["config", "export", "--to", &file.display().to_string()]);
    assert!(std::fs::read_to_string(&file).expect("the exported file").contains("sidebar = \"tree\""));

    let session = started();
    assert_ne!(setting(&session, "sidebar"), Value::from("tree"));
    session.says(&["config", "import", &file.display().to_string(), "--yes"]);
    assert_eq!(setting(&session, "sidebar"), Value::from("tree"));
}

#[test]
fn import_from_standard_input_needs_yes() {
    let text = colleague_with_a_tree().says(&["config", "export"]);
    let session = started();
    let before = setting(&session, "sidebar");
    let out = session.fed(&["config", "import", "-"], &text);

    assert!(!out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("add --yes"), "{out:?}");
    assert_eq!(setting(&session, "sidebar"), before);
}

#[test]
fn preview_prints_the_changes_and_imports_nothing() {
    let (_colleague, path) = exported_tree();
    let session = started();
    let before = setting(&session, "sidebar");

    let out = session.cli(&["config", "import", &path, "--preview"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(text.contains("sidebar: projects_on_top → tree"), "{text}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("add --yes"), "{out:?}");
    assert_eq!(setting(&session, "sidebar"), before);

    let agents = session.says(&["config", "import", &path, "--preview", "--only", "agents"]);
    assert!(!agents.contains("→ tree"), "{agents}");
    let unknown = session.cli(&["config", "import", &path, "--preview", "--only", "nope"]);
    assert_eq!(unknown.status.code(), Some(1), "{unknown:?}");
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("is not a section"), "{unknown:?}");
    assert_eq!(setting(&session, "sidebar"), before);
}

#[test]
fn preview_json_from_standard_input_exits_0_without_applying() {
    let text = colleague_with_a_tree().says(&["config", "export"]);
    let session = started();
    let before = setting(&session, "sidebar");
    let out = session.fed(&["config", "import", "-", "--preview", "--json", "--only", "interface"], &text);

    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let preview: Value = serde_json::from_slice(&out.stdout).expect("a preview");
    let sections = preview["sections"].as_array().expect("sections");
    assert_eq!(sections.len(), 1, "{preview}");
    let changes = sections[0]["changes"].as_array().expect("changes");
    assert!(changes.iter().any(|change| change["key"] == "sidebar" && change["to"] == "tree"), "{preview}");
    assert_eq!(setting(&session, "sidebar"), before);
}

#[test]
fn a_preview_of_a_file_that_is_not_a_setup_exits_1() {
    let session = started();
    let file = session.dir.join("nope.toml");
    std::fs::write(&file, "sidebar = \"tree\"\n").expect("write");
    let before = setting(&session, "sidebar");
    let out = session.cli(&["config", "import", &file.display().to_string(), "--preview", "--json"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a cornercase setup"), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    assert_eq!(setting(&session, "sidebar"), before);
}

#[test]
fn preview_and_yes_cannot_be_combined() {
    let session = Session::new();
    let out = session.cli(&["config", "import", "team.toml", "--preview", "--yes"]);
    let err = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(err.contains("--preview") && err.contains("--yes"), "{err}");
}

fn exported_tree() -> (Arc<Session>, String) {
    let colleague = colleague_with_a_tree();
    let file = colleague.dir.join("setup.toml");
    colleague.says(&["config", "export", "--to", &file.display().to_string()]);
    (colleague, file.display().to_string())
}
