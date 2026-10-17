use serde_json::Value;

use super::*;

fn setting(session: &Session, key: &str) -> Value {
    let described: Value = serde_json::from_str(&session.says(&["config", "--json"])).expect("a description");
    let found = described["settings"].as_array().expect("settings").iter().find(|s| s["key"] == key);
    found.expect("a setting")["value"].clone()
}

fn wait_for_setting(session: &Session, key: &str, expected: Value) {
    let deadline = Instant::now() + TIMEOUT;
    while setting(session, key) != expected {
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
    wait_for_setting(&session, "counts", Value::Bool(true));

    let refused = session.cli(&["config", "set", "worktrees_dir", "relative"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("use an absolute path"), "{refused:?}");
}
