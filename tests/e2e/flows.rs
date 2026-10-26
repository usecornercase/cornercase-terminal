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

const DRAFT: &str = r#"{"name": "Mine", "orchestrator": {"agent": "claude", "prompt": "Run it."}, "roles": [{"name": "coder", "agent": "claude", "edits": true, "prompt": "Write it."}], "stages": [{"roles": ["coder"]}]}"#;

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

fn risky_flow() -> String {
    FLOW.replace("edits = true", "edits = true\nargs = [\"--verbose\"]\nenv = { TOKEN = \"demo\" }")
}

#[test]
fn an_import_check_from_standard_input_gives_roles_and_risks_without_saving() {
    let app = Harness::start();
    let before = json(&app.session, &["flow", "list", "--json"]);
    let out = app
        .session
        .fed(&["flow", "import", "-", "--check", "--json", "--name", "Ours", "--to", "project"], &risky_flow());
    let check = answered(&out);

    assert_eq!((check["name"].as_str(), check["origin"].as_str()), (Some("Ours"), Some("project")));
    assert_eq!(
        (check["roles"][0]["role"].as_str(), check["roles"][1]["agent"].as_str()),
        (Some("orchestrator"), Some("claude"))
    );
    assert_eq!(check["roles"][1]["edits"], true);
    let risks = check["check"].as_str().expect("risks");
    assert!(risks.contains("TOKEN=demo") && risks.contains("--verbose") && risks.contains("without asking"));
    assert!(check["path"].is_null());
    assert_eq!(json(&app.session, &["flow", "list", "--json"]), before);
    assert!(!app.session.repo.join(".cornercase/flows").exists());
}

#[test]
fn an_import_check_from_a_file_prints_text_and_a_replacement_keeps_the_saved_flow() {
    let app = Harness::start();
    let file = app.session.dir.join("mine.toml");
    std::fs::write(&file, risky_flow()).expect("a flow file");
    let file = file.display().to_string();
    let args = ["flow", "import", &file, "--check"];
    let text = app.session.says(&args);
    assert!(text.contains("orchestrator: claude") && text.contains("coder: claude (edits)"), "{text}");
    assert!(text.contains("TOKEN=demo") && text.contains("nothing was imported"), "{text}");
    let saved = json(&app.session, &["flow", "import", &file, "--yes", "--json"]);
    let path = saved["path"].as_str().expect("a saved path");
    let before = std::fs::read_to_string(path).expect("saved flow");
    std::fs::write(&file, risky_flow().replace("Write it.", "Changed.")).expect("a changed flow");
    let refused = app.session.cli(&args);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let check = json(&app.session, &["flow", "import", &file, "--check", "--replace", "--json"]);
    assert_eq!(check["replaced"], true);
    assert_eq!(std::fs::read_to_string(path).expect("unchanged flow"), before);
}

#[test]
fn an_import_check_refuses_bad_text_and_a_flow_that_cannot_run() {
    let app = Harness::start();
    for text in ["name = = 1".to_string(), FLOW.replace("agent = \"claude\"", "agent = \"unknown\"")] {
        let out = app.session.fed(&["flow", "import", "-", "--check", "--json"], &text);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
    }
    assert!(!names(&json(&app.session, &["flow", "list", "--json"]), "flows").contains(&"Mine".into()));
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

#[test]
fn a_default_and_an_order_are_set_and_listed() {
    let app = Harness::start();

    let default = app.session.says(&["flow", "default", "quick"]);
    let order = app.session.says(&["flow", "order", "Quick", "Arena"]);
    let list = json(&app.session, &["flow", "list", "--json"]);

    assert_eq!(default, "the start dialog opens on Quick now");
    assert!(order.starts_with("the flows are in this order now: Quick, Arena"), "{order}");
    assert_eq!(&names(&list, "flows")[..2], ["Quick", "Arena"]);
    let flows = list["flows"].as_array().expect("flows");
    let marked: Vec<&Value> = flows.iter().filter(|f| f["default"] == true).collect();
    assert_eq!(marked.len(), 1, "{list}");
    assert_eq!(marked[0]["name"], "Quick");
    let text = app.session.says(&["flow", "list"]);
    assert!(text.starts_with("Quick (built in, default)\n"), "{text}");
}

#[test]
fn a_default_and_an_order_are_cleared_and_refused() {
    let app = Harness::start();
    app.session.says(&["flow", "default", "Quick"]);
    app.session.says(&["flow", "order", "Quick"]);
    assert!(app.session.says(&["flow", "default", "Quick"]).contains("was already the default flow"));

    let cleared = app.session.says(&["flow", "default", "--clear"]);
    let back = app.session.says(&["flow", "order", "--clear"]);
    let refused = app.session.cli(&["flow", "default", "Nope"]);

    assert!(cleared.starts_with("no default flow"), "{cleared}");
    assert!(back.starts_with("the flows are in this order now: Arena"), "{back}");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("there is no flow named `Nope`"), "{refused:?}");
}

fn saving(session: &Session, args: &[&str], draft: &str) -> std::process::Output {
    let mut all = vec!["flow", "save", "-"];
    all.extend_from_slice(args);
    session.fed(&all, draft)
}

fn answered(out: &std::process::Output) -> Value {
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("a JSON answer")
}

fn listed<'a>(list: &'a Value, name: &str) -> &'a Value {
    list["flows"].as_array().expect("flows").iter().find(|f| f["name"] == name).expect("a listed flow")
}

#[test]
fn a_listed_flow_is_saved_back_from_its_json_and_renamed_by_its_file() {
    let app = Harness::start();
    let first = answered(&saving(&app.session, &["--json"], DRAFT));
    assert_eq!((first["saved"].as_bool(), first["origin"].as_str()), (Some(true), Some("user")), "{first}");
    let old = first["path"].as_str().expect("a path").to_string();

    let list = json(&app.session, &["flow", "list", "--json"]);
    let mut draft = listed(&list, "Mine")["flow"].clone();
    draft["name"] = "Ours".into();
    draft["roles"][0]["prompt"] = "Write it well.".into();
    let draft = draft.to_string();
    let check = answered(&saving(&app.session, &["--check", "--json"], &draft));
    assert_eq!(
        (check["saved"].as_bool(), check["problems"].as_array().map(Vec::len)),
        (Some(false), Some(0)),
        "{check}"
    );
    assert!(check["path"].is_null(), "{check}");
    let done = answered(&saving(&app.session, &["--path", &old, "--json"], &draft));

    assert_eq!(done["origin"], "user", "{done}");
    assert!(!std::path::Path::new(&old).exists());
    let list = json(&app.session, &["flow", "list", "--json"]);
    assert_eq!(listed(&list, "Ours")["flow"]["roles"][0]["prompt"], "Write it well.");
    assert!(!names(&list, "flows").contains(&"Mine".to_string()), "{list}");
}

#[test]
fn a_draft_with_problems_or_a_stray_key_is_refused_and_a_good_one_is_saved_from_a_file() {
    let app = Harness::start();
    let mut empty: Value = serde_json::from_str(DRAFT).expect("a draft");
    empty["stages"] = Value::Array(Vec::new());
    let empty = empty.to_string();

    let check = saving(&app.session, &["--check"], &empty);
    let refused = saving(&app.session, &[], &empty);
    let mut typo: Value = serde_json::from_str(DRAFT).expect("a draft");
    typo["roles"][0]["promt"] = "x".into();
    let stray = saving(&app.session, &[], &typo.to_string());

    assert!(check.status.success(), "{check:?}");
    assert!(String::from_utf8_lossy(&check.stdout).contains("cannot run: the flow has no stages"), "{check:?}");
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("the flow has problems"), "{refused:?}");
    assert!(!stray.status.success());
    assert!(String::from_utf8_lossy(&stray.stderr).contains("roles.0.promt"), "{stray:?}");
    let file = app.session.dir.join("draft.json");
    std::fs::write(&file, DRAFT).expect("a draft file");
    let said = app.session.says(&["flow", "save", &file.display().to_string()]);
    assert!(said.starts_with("saved Mine into yours: "), "{said}");
}
