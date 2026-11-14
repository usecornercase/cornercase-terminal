use serde_json::Value;

use super::*;

const STREAM: &str = r#"{"type":"system","subtype":"init","model":"claude-opus-5-5","session_id":"s1"}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","name":"Bash","input":{"command":"cornercase status --json"}}]}}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"hello from the bot"}]}}
{"type":"result","subtype":"success","is_error":false,"result":"hello from the bot","session_id":"s1","usage":{"input_tokens":3,"output_tokens":4}}"#;

fn started() -> Arc<Session> {
    started_running("")
}

fn started_running(before: &str) -> Arc<Session> {
    let session = Session::new();
    let claude = session.dir.join("claude-bot");
    write_executable(
        &claude,
        &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.args\"\n{before}\ncat <<'EOF'\n{STREAM}\nEOF\n"),
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

const HEAD: &str = "a bot can only type into a pane that runs an agent";

fn shell_pane(session: &Session) -> String {
    let work = session.dir.join("work");
    std::fs::create_dir_all(&work).expect("a folder");
    let project: u64 = session.says(&["open", &work.display().to_string()]).parse().expect("an id");
    let report: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("json");
    let opened = report["projects"].as_array().and_then(|all| all.iter().find(|p| p["id"] == project));
    opened.expect("the project")["workspaces"][0]["tabs"][0]["panes"][0]["id"].to_string()
}

#[test]
fn a_bot_cannot_type_into_a_shell_pane_but_you_can() {
    let session = started_running(
        "cornercase send --pane \"$(cat \"$0.pane\")\" --enter \"touch $0.typed\" > \"$0.out\" 2> \"$0.err\"\necho $? > \"$0.status\"",
    );
    let pane = shell_pane(&session);
    std::fs::write(session.dir.join("claude-bot.pane"), &pane).expect("the pane");
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    session.says(&["bot", "say", "prs", "go", "--wait"]);

    let status = std::fs::read_to_string(session.dir.join("claude-bot.status")).expect("the command ran");
    let stderr = std::fs::read_to_string(session.dir.join("claude-bot.err")).expect("its stderr");
    assert_eq!(status.trim(), "1", "{stderr}");
    assert!(stderr.contains(HEAD), "{stderr}");
    assert!(!session.dir.join("claude-bot.typed").exists(), "the bot typed into the shell");

    let yours = session.dir.join("typed-by-you");
    session.says(&["send", "--pane", &pane, "--enter", &format!("touch {}", yours.display())]);
    session.wait_for_file("typed-by-you", "the shell ran what you typed", |_| yours.exists());
}

#[test]
fn a_command_whose_environment_names_a_bot_is_refused_by_the_server() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    let pane = shell_pane(&session);

    let sent =
        session.command().env("CORNERCASE_BOT", "prs").args(["send", "--pane", &pane, "x"]).output().expect("run");

    assert!(refused(&sent, 1, HEAD), "{sent:?}");
}
