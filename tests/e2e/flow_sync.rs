use serde_json::{Value, json};

use super::flows::{FLOW, json, names};
use super::machines::{VPS, linked_to_one};
use super::*;

fn listed(session: &Session) -> Value {
    json(session, &["flow", "list", "--json"])
}

fn import_mine(here: &Session) {
    let file = here.dir.join("mine.toml");
    std::fs::write(&file, FLOW).expect("a flow file");
    here.says(&["flow", "import", &file.display().to_string(), "--yes"]);
}

fn mine_in(session: &Session) -> Value {
    let flows = listed(session)["flows"].as_array().cloned().unwrap_or_default();
    flows.into_iter().find(|flow| flow["name"] == "Mine").unwrap_or_default()
}

fn has(session: &Session, key: &str, name: &str) -> bool {
    names(&listed(session), key).contains(&name.to_string())
}

#[test]
fn a_flow_made_or_deleted_here_is_made_or_deleted_on_a_machine_that_syncs_flows() {
    let (machine, here) = linked_to_one();
    let edited = here.says(&["machines", "edit", VPS, "--sync-flows"]);
    assert!(edited.contains("syncs flows"), "{edited}");

    import_mine(&here);
    wait_until("the flow reaches the machine", || has(&machine, "flows", "Mine"));
    here.says(&["flow", "hide", "Arena"]);
    wait_until("the built-in is deleted there", || has(&machine, "hidden", "Arena"));

    let synced = json(&here, &["flow", "sync", "--json"]);
    assert_eq!((&synced["machines"][0]["machine"], &synced["machines"][0]["error"]), (&json!(VPS), &Value::Null));
    assert_eq!(mine_in(&here)["synced"], json!([VPS]));
    assert!(mine_in(&machine)["changed"].is_string());

    here.says(&["flow", "delete", "Mine"]);
    wait_until("the flow is deleted there", || !has(&machine, "flows", "Mine"));
}

#[test]
fn a_machine_that_does_not_sync_flows_gets_none_and_flow_sync_says_how_to_turn_it_on() {
    let (machine, here) = linked_to_one();
    import_mine(&here);

    let refused = here.cli(&["flow", "sync", "--machine", VPS]);

    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--sync-flows"), "{refused:?}");
    assert!(!has(&machine, "flows", "Mine"));
}
