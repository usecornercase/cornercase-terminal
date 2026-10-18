use serde_json::Value;

use super::*;

fn json(session: &Session, args: &[&str]) -> Value {
    serde_json::from_str(&session.says(args)).expect("a JSON answer")
}

fn boot(session: &Session) -> String {
    let status = json(session, &["status", "--json"]);
    status["boot"].as_str().expect("a boot in status").to_string()
}

#[test]
fn contract_describes_the_running_server_and_its_boot() {
    let app = Harness::start();

    let contract = json(&app.session, &["contract", "--json"]);

    let keys: Vec<&str> = contract.as_object().expect("an object").keys().map(String::as_str).collect();
    assert_eq!(keys, ["boot", "contract", "features", "operations", "streams", "version"], "{contract}");
    assert_eq!(contract["contract"], 1);
    assert_eq!(contract["version"], cornercase::version::CURRENT);
    assert!(!contract["boot"].as_str().expect("a boot").is_empty(), "{contract}");
    assert_eq!(contract["boot"].as_str(), Some(boot(&app.session).as_str()));
    let operations = contract["operations"].as_array().expect("operations");
    assert!(operations.iter().any(|op| op["name"] == "contract" && op["available"] == true), "{contract}");
    assert!(operations.iter().any(|op| op["name"] == "status"), "{contract}");
    assert!(operations.iter().any(|op| op["name"] == "start-server" && op["available"] == true), "{contract}");
    assert!(
        contract["streams"].as_array().expect("streams").iter().any(|s| s["name"] == "events" && s["version"] == 1),
        "{contract}"
    );
    for name in [
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
    ] {
        assert!(operations.iter().any(|op| op["name"] == name && op["available"] == true), "{name}: {contract}");
    }
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "views"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "host-events"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "theme"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "notices"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "config"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "close-unlock"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "issues"), "{contract}");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "todo-panel"), "{contract}");
    assert!(
        contract["streams"].as_array().expect("streams").iter().any(|s| s["name"] == "view" && s["version"] == 1),
        "{contract}"
    );
    assert!(
        contract["streams"].as_array().expect("streams").iter().any(|s| s["name"] == "surface" && s["version"] == 1),
        "{contract}"
    );
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
