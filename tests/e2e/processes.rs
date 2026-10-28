use serde_json::Value;

use super::*;

fn processes(session: &Session, args: &[&str]) -> Value {
    let args = [&["processes", "--json", "--interval", "100"], args].concat();
    serde_json::from_str(&session.says(&args)).expect("json")
}

fn pane_of(list: &Value, id: u64) -> Option<&Value> {
    list["panes"].as_array()?.iter().find(|pane| pane["pane"] == id)
}

fn sleeper(list: &Value, id: u64) -> Option<Value> {
    let procs = pane_of(list, id)?["processes"].as_array()?;
    procs.iter().find(|proc| proc["name"] == "sleep").cloned()
}

#[test]
fn a_tab_lists_its_sleeper_under_its_shell_and_a_pane_narrows_the_list() {
    let app = Harness::start();
    let id: u64 = app.session.says(&["new-tab", "--name", "sleeper", "--", "sleep", "30"]).parse().expect("a pane id");

    let mut found = None;
    wait_until("the sleeper is listed", || {
        found = sleeper(&processes(&app.session, &[]), id);
        found.is_some()
    });

    let found = found.expect("the sleeper");
    let list = processes(&app.session, &[]);
    let pane = pane_of(&list, id).expect("the pane");
    assert!(found["memory"].as_u64().is_some_and(|bytes| bytes > 0), "{found}");
    assert!(pane["processes"].as_array().is_some_and(|procs| !procs.is_empty()), "{pane}");
    assert!(list["cornercase"]["pid"].as_u64().is_some(), "{list}");
    let narrowed = processes(&app.session, &["--pane", &id.to_string()]);
    assert_eq!(narrowed["panes"].as_array().map(Vec::len), Some(1), "{narrowed}");
    assert_eq!(narrowed["cornercase"], Value::Null);
    let text = app.session.says(&["processes", "--interval", "100"]);
    assert!(text.contains(&format!("pane {id}")) && text.contains("sleep 30") && text.contains("total: "), "{text}");
}

#[test]
fn processes_stop_ends_a_program_refuses_a_shell_and_a_gone_pid() {
    let app = Harness::start();
    let id: u64 = app.session.says(&["new-tab", "--name", "sleeper", "--", "sleep", "30"]).parse().expect("a pane id");
    let mut found = None;
    wait_until("the sleeper is listed", || {
        found = sleeper(&processes(&app.session, &[]), id);
        found.is_some()
    });
    let pid = found.expect("the sleeper")["pid"].as_i64().expect("a pid").to_string();

    let stopped: Value = serde_json::from_str(&app.session.says(&["processes", "stop", &pid, "--json"])).expect("json");

    assert_eq!((stopped["outcome"].as_str(), stopped["pane"].as_u64()), (Some("stopped"), Some(id)));
    let again = app.session.cli(&["processes", "stop", &pid]);
    assert_eq!(again.status.code(), Some(1), "{again:?}");
    assert!(String::from_utf8_lossy(&again.stderr).contains("no running process"), "{again:?}");
    let list = processes(&app.session, &[]);
    let shell = list["panes"][0]["pid"].as_i64().expect("a shell pid").to_string();
    let refused = app.session.cli(&["processes", "stop", &shell]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("close the pane instead"), "{refused:?}");
}
