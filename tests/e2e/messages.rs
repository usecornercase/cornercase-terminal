use serde_json::{Value, json};

use super::commands::{agent_app, send_and_finish, start_and_finish};
use super::*;

fn ids(thread: &Value) -> Vec<u64> {
    thread["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|message| message["id"].as_u64().expect("an id"))
        .collect()
}

fn read(session: &Session, pane: &str, args: &[&str]) -> Value {
    let args = [&["read", "--messages", "--pane", pane, "--json"][..], args].concat();
    serde_json::from_str(&session.says(&args)).expect("json")
}

#[test]
fn an_agents_conversation_is_read_a_page_at_a_time() {
    let app = agent_app();
    let session = &app.session;
    let started = start_and_finish(session, "fix the login");
    let pane = started.lines().next().expect("the pane").to_string();
    send_and_finish(session, &pane, &["--timeout", "30"], "add tests");

    let whole = read(session, &pane, &[]);
    let newest = read(session, &pane, &["--last", "1"]);
    let earlier = read(session, &pane, &["--before", "2"]);
    let text = session.cli(&["read", "--messages", "--pane", &pane, "--last", "1"]);
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();
    let no_agent = session.cli(&["read", "--messages", "--pane", &shell]);

    assert_eq!(
        (&whole["agent"], &whole["messages"][0]["text"], &whole["messages"][1]["text"]),
        (&json!("claude"), &json!("said: fix the login"), &json!("said: add tests"))
    );
    assert_eq!(whole["messages"][1]["role"], "assistant");
    assert_eq!((ids(&whole), &whole["more_before"]), (vec![1, 2], &json!(false)));
    assert_eq!((ids(&newest), &newest["more_before"]), (vec![2], &json!(true)));
    assert_eq!((ids(&earlier), &earlier["more_before"]), (vec![1], &json!(false)));
    assert_eq!(String::from_utf8_lossy(&text.stdout), "2  assistant  2026-10-08T12:00:02.000Z\nsaid: add tests\n");
    assert_eq!(String::from_utf8_lossy(&text.stderr), "earlier messages: --before 2\n");
    assert!(refused(&no_agent, 1, &format!("pane {shell} runs no agent")), "{no_agent:?}");
}
