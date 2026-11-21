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
fn flow_sync_sends_a_flow_of_yours_and_nothing_else_changes_there() {
    let (machine, here) = linked_to_one();
    import_mine(&here);
    here.says(&["flow", "hide", "Arena"]);
    assert!(!has(&machine, "flows", "Mine"), "nothing is sent on its own");

    let synced = json(&here, &["flow", "sync", "Mine", "--json"]);

    assert_eq!((&synced["machines"][0]["machine"], &synced["machines"][0]["sent"]), (&json!(VPS), &json!(["Mine"])));
    assert!(mine_in(&machine)["changed"].is_string());
    assert!(!has(&machine, "hidden", "Arena"), "hiding a built-in stays on this computer");

    here.says(&["flow", "delete", "Mine"]);
    let again = json(&here, &["flow", "sync", "--json"]);
    assert_eq!(again["machines"][0]["error"], Value::Null);
    assert!(has(&machine, "flows", "Mine"), "nothing is deleted there");
}

#[test]
fn flow_sync_of_a_flow_that_is_not_yours_is_refused_and_sends_nothing() {
    let (machine, here) = linked_to_one();
    import_mine(&here);

    let refused = here.cli(&["flow", "sync", "Nope"]);

    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("no flow of yours"), "{refused:?}");
    assert!(!has(&machine, "flows", "Mine"));
}

#[test]
fn flow_sync_reports_a_file_it_cannot_read_but_never_logs_what_the_file_holds() {
    let (_machine, here) = linked_to_one();
    import_mine(&here);
    let mine = mine_in(&here);
    let folder = std::path::Path::new(mine["path"].as_str().expect("the file of Mine")).parent().expect("its folder");
    std::fs::write(folder.join("leak.toml"), "name = \"Leak\"\nversion = \"sk-left-\"\n").expect("a broken flow file");

    let out = here.cli(&["flow", "sync", "--json"]);

    assert!(!out.status.success(), "{out:?}");
    let synced: Value = serde_json::from_slice(&out.stdout).expect("a JSON answer");
    let failed = &synced["machines"][0]["failed"][0];
    assert_eq!(failed["name"], "leak.toml");
    assert!(failed["error"].as_str().is_some_and(|why| why.contains("sk-left-")), "the answer says why: {failed}");
    here.wait_for_file("server.log", "the sync is logged", |log| log.contains("flow synced"));
    let log = here.says(&["logs", "-n", "1000"]);
    assert!(log.contains("flow left out of a sync") && log.contains("leak.toml"), "{log}");
    assert!(!log.contains("sk-left-"), "the log holds what the file holds:\n{log}");
}
