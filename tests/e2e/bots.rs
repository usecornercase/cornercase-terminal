use serde_json::Value;

use super::*;

const STREAM: &str = r#"{"type":"system","subtype":"init","model":"claude-opus-5-5","session_id":"s1"}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","name":"Bash","input":{"command":"cornercase status --json"}}]}}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"hello from the bot"}]}}
{"type":"result","subtype":"success","is_error":false,"result":"hello from the bot","session_id":"s1","usage":{"input_tokens":3,"output_tokens":4}}"#;

pub(super) fn started() -> Arc<Session> {
    let session = Session::new();
    let claude = session.dir.join("claude-bot");
    write_executable(
        &claude,
        &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.args\"\ncat <<'EOF'\n{STREAM}\nEOF\n"),
    );
    let config = serde_json::json!({"agent_commands": agent_commands(&[("claude", &claude)]), "agent": "claude"});
    std::fs::write(session.dir.join("config.json"), config.to_string()).expect("write the config");
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");
    session
}

#[test]
fn a_bot_is_made_asked_and_read_back_over_the_socket() {
    let session = started();

    assert_eq!(session.says(&["bot", "new", "PRs", "--agent", "claude", "--about", "review my PRs"]), "prs");
    assert!(!session.dir.join("claude-bot.args").exists(), "making a bot starts nothing");
    assert_eq!(session.says(&["bot", "say", "prs", "which are blocked?", "--wait"]), "hello from the bot");

    let list: Value = serde_json::from_str(&session.says(&["bot", "list", "--json"])).expect("json");
    assert_eq!((list["bots"][0]["id"].as_str(), list["bots"][0]["state"].as_str()), (Some("prs"), Some("idle")));
    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.starts_with("you · "), "{history}");
    assert!(history.contains("▸ cornercase status --json") && history.contains("hello from the bot"), "{history}");
    assert!(session.says(&["bot", "agents"]).contains("claude"));
    let args = std::fs::read_to_string(session.dir.join("claude-bot.args")).expect("the agent ran");
    assert!(args.contains("--setting-sources user") && args.contains("which are blocked?"), "{args}");
}

#[test]
fn inside_a_bot_a_command_must_name_its_place_and_the_server_cannot_be_stopped() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    let start = session.command().env("CORNERCASE_BOT", "prs").args(["start", "--prompt", "x"]).output().expect("run");
    assert!(refused(&start, 2, "a bot runs in no pane"), "{start:?}");
    let kill = session.command().env("CORNERCASE_BOT", "prs").arg("kill-server").output().expect("run");
    assert!(refused(&kill, 2, "a bot cannot stop its own server"), "{kill:?}");
    assert!(session.says(&["status", "--json"]).contains("\"projects\""));
}

#[test]
fn a_bot_made_from_the_form_answers_in_its_chat() {
    let session = started();
    let mut ui = Harness::open(Arc::clone(&session), ROWS, COLS);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));

    ui.pick_in_more("new bot…");
    ui.wait_for("the form opens", |s| s.contains("New bot") && s.contains("What it is for") || s.contains("For"));
    ui.send(b"prs");
    ui.click(ui.position_of("create"));
    ui.wait_for("its empty chat shows", |s| s.contains("Message prs…"));
    ui.send(b"which are blocked?\r");
    ui.wait_for("its answer is drawn", |s| s.contains("hello from the bot") && s.contains("cornercase status --json"));

    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.starts_with("you · ") && history.contains("which are blocked?"), "{history}");
}
