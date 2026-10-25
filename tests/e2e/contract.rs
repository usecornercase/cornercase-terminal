use serde_json::Value;

use super::*;

fn json(session: &Session, args: &[&str]) -> Value {
    serde_json::from_str(&session.says(args)).expect("a JSON answer")
}

fn boot(session: &Session) -> String {
    let status = json(session, &["status", "--json"]);
    status["boot"].as_str().expect("a boot in status").to_string()
}

fn operation<'a>(contract: &'a Value, name: &str) -> Option<&'a Value> {
    contract["operations"].as_array().expect("operations").iter().find(|op| op["name"] == name)
}

fn has_available(contract: &Value, name: &str) -> bool {
    operation(contract, name).is_some_and(|op| op["available"] == true)
}

fn has_feature(contract: &Value, name: &str) -> bool {
    contract["features"].as_array().expect("features").iter().any(|f| f == name)
}

fn has_stream(contract: &Value, name: &str) -> bool {
    contract["streams"].as_array().expect("streams").iter().any(|s| s["name"] == name && s["version"] == 1)
}

fn assert_header(contract: &Value, session: &Session) {
    let keys: Vec<&str> = contract.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(keys, ["boot", "contract", "features", "operations", "streams", "version"], "{contract}");
    assert_eq!(contract["contract"], 1);
    assert_eq!(contract["version"], cornercase::version::CURRENT);
    assert!(!contract["boot"].as_str().expect("a boot").is_empty(), "{contract}");
    assert_eq!(contract["boot"].as_str(), Some(boot(session).as_str()));
}

const OPERATIONS: [&str; 38] = [
    "contract",
    "start-server",
    "surface",
    "view",
    "view-set",
    "input",
    "scroll",
    "selection",
    "resize",
    "ratio",
    "link",
    "theme",
    "go",
    "redraw",
    "config",
    "move",
    "move-pane",
    "collapse",
    "expand",
    "group-new",
    "group-style",
    "group-delete",
    "removal-check",
    "close-agent",
    "app",
    "usage",
    "plugin-list",
    "plugin-actions",
    "plugin-run",
    "flow-save",
    "flow-delete",
    "flow-hide",
    "flow-restore",
    "flow-default",
    "flow-order",
    "flow-runs",
    "flow-messages",
    "flow-files",
];

fn assert_operations(contract: &Value) {
    assert!(operation(contract, "status").is_some(), "{contract}");
    for name in OPERATIONS {
        assert!(has_available(contract, name), "{name}: {contract}");
    }
}

fn assert_updates_only_in_release_builds(contract: &Value) {
    for name in ["update-check", "update-install"] {
        let operation = operation(contract, name).unwrap_or_else(|| panic!("{name}: {contract}"));
        assert_eq!(operation["available"], !cfg!(debug_assertions), "{name}: {contract}");
        if cfg!(debug_assertions) {
            let reason = operation["reason"].as_str().unwrap_or_default();
            assert!(reason.contains("development build"), "{name}: {contract}");
        }
    }
}

fn assert_features_and_streams(contract: &Value) {
    let features = ["views", "host-events", "theme", "notices", "config", "close-unlock", "issues"];
    for feature in features
        .into_iter()
        .chain(["todo-panel", "agent-folds", "changes", "close-agent-tabs", "queue-undo"])
        .chain(["focus-reveal"])
    {
        assert!(has_feature(contract, feature), "{feature}: {contract}");
    }
    let streams = ["events", "app", "changes", "flow-runs", "flow-messages", "view", "surface"];
    for stream in streams {
        assert!(has_stream(contract, stream), "{stream}: {contract}");
    }
}

#[test]
fn contract_describes_the_running_server_and_its_boot() {
    let app = Harness::start();

    let contract = json(&app.session, &["contract", "--json"]);

    assert_header(&contract, &app.session);
    assert_operations(&contract);
    assert_updates_only_in_release_builds(&contract);
    assert_features_and_streams(&contract);
}

#[test]
fn a_restart_gives_the_server_another_boot() {
    let mut app = Harness::start();
    let before = boot(&app.session);

    app.restart_server();

    let after = boot(&app.session);
    let contract = json(&app.session, &["contract", "--json"]);
    assert_ne!(before, after);
    assert_eq!(contract["boot"].as_str(), Some(after.as_str()));
}
