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
