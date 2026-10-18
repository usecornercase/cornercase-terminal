use serde_json::Value;

use super::*;

const FLOW: &str = r#"# shared
version = 1
name = "Mine"

[orchestrator]
name = "orchestrator"
agent = "claude"
prompt = "Run it."

[[role]]
name = "coder"
agent = "claude"
edits = true
prompt = "Write it."

[[stage]]
roles = ["coder"]
"#;

fn json(session: &Session, args: &[&str]) -> Value {
    serde_json::from_str(&session.says(args)).expect("a JSON answer")
}

fn names(list: &Value, key: &str) -> Vec<String> {
    list[key].as_array().expect("a list").iter().filter_map(|f| f["name"].as_str().map(str::to_string)).collect()
}

#[test]
fn the_list_gives_each_flow_whole_and_the_reserved_names() {
    let app = Harness::start();

    let list = json(&app.session, &["flow", "list", "--json"]);

    let quick = list["flows"].as_array().expect("flows").iter().find(|f| f["name"] == "Quick").expect("Quick");
    assert_eq!(quick["flow"]["name"], "Quick");
    assert_eq!(quick["flow"]["orchestrator"]["name"], "orchestrator");
    assert!(quick["flow"]["stages"].as_array().is_some_and(|stages| !stages.is_empty()), "{quick}");
    assert_eq!(quick["edited"], false);
    assert!(list["builtins"].as_array().expect("builtins").iter().any(|name| name == "Quick"), "{list}");
    assert_eq!(list["hidden"].as_array().map(Vec::len), Some(0));
}

#[test]
fn a_built_in_is_hidden_refused_and_restored() {
    let app = Harness::start();

    let said = app.session.says(&["flow", "hide", "Quick"]);
    let list = json(&app.session, &["flow", "list", "--json"]);

    assert!(said.contains("deleted the built-in flow Quick"), "{said}");
    assert_eq!(names(&list, "hidden"), ["Quick"]);
    assert!(!names(&list, "flows").contains(&"Quick".to_string()), "{list}");
    let run = app.session.cli(&["flow", "run", "Quick", "--prompt", "x"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("was deleted"), "{run:?}");
    let text = app.session.says(&["flow", "list"]);
    assert!(text.contains("deleted built-in flows: Quick"), "{text}");

    let restored = app.session.says(&["flow", "restore", "Quick"]);

    assert!(restored.contains("restored the built-in flow Quick"), "{restored}");
    assert!(names(&json(&app.session, &["flow", "list", "--json"]), "flows").contains(&"Quick".to_string()));
}

#[test]
fn an_imported_flow_is_deleted_by_name() {
    let app = Harness::start();
    let file = app.session.dir.join("mine.toml");
    std::fs::write(&file, FLOW).expect("a flow file");
    app.session.says(&["flow", "import", &file.display().to_string(), "--yes"]);
    assert!(names(&json(&app.session, &["flow", "list", "--json"]), "flows").contains(&"Mine".to_string()));

    let said = app.session.says(&["flow", "delete", "Mine"]);

    assert!(said.starts_with("deleted Mine (yours):"), "{said}");
    assert!(!names(&json(&app.session, &["flow", "list", "--json"]), "flows").contains(&"Mine".to_string()));
    let again = app.session.cli(&["flow", "delete", "Quick"]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("flow hide Quick"), "{again:?}");
}

#[test]
fn with_no_run_there_is_nothing_to_follow() {
    let app = Harness::start();

    let runs = json(&app.session, &["flow", "runs", "--json"]);
    let text = app.session.says(&["flow", "runs"]);
    let messages = app.session.cli(&["flow", "messages"]);
    let files = app.session.cli(&["flow", "files"]);

    assert_eq!(runs["runs"].as_array().map(Vec::len), Some(0), "{runs}");
    assert_eq!(runs["boot"], json(&app.session, &["status", "--json"])["boot"]);
    assert_eq!(text, "no flow is running");
    assert!(!messages.status.success());
    assert!(String::from_utf8_lossy(&messages.stderr).contains("runs no flow"), "{messages:?}");
    assert!(!files.status.success());
    assert!(String::from_utf8_lossy(&files.stderr).contains("no run folder"), "{files:?}");
}

#[test]
fn the_runs_stream_starts_with_the_current_state() {
    let app = Harness::start();
    let mut child = app.session.spawn(&["flow", "runs", "--follow"]);
    let stdout = child.stdout.take().expect("a pipe");
    let mut first = String::new();

    std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut first).expect("a line");
    let _ = child.kill();
    let _ = child.wait();

    let line: Value = serde_json::from_str(&first).expect("a JSON line");
    assert_eq!(line["runs"].as_array().map(Vec::len), Some(0), "{first}");
    assert!(line["revision"].as_u64().is_some_and(|revision| revision >= 1), "{first}");
}
