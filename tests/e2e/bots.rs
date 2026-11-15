use serde_json::Value;

use super::*;

const STREAM: &str = r#"{"type":"system","subtype":"init","model":"claude-opus-5-5","session_id":"s1"}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"tool_use","name":"Bash","input":{"command":"cornercase status --json"}}]}}
{"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"hello from the bot"}]}}
{"type":"result","subtype":"success","is_error":false,"result":"hello from the bot","session_id":"s1","usage":{"input_tokens":3,"output_tokens":4}}"#;

pub(super) fn with_agent() -> Arc<Session> {
    with_agent_running("")
}

fn with_agent_running(before: &str) -> Arc<Session> {
    let session = Session::new();
    let claude = session.dir.join("claude-bot");
    write_executable(
        &claude,
        &format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$0.args\"\n{before}\ncat <<'EOF'\n{STREAM}\nEOF\n"),
    );
    let config = serde_json::json!({"agent_commands": agent_commands(&[("claude", &claude)]), "agent": "claude"});
    std::fs::write(session.dir.join("config.json"), config.to_string()).expect("write the config");
    session
}

pub(super) fn started() -> Arc<Session> {
    started_running("")
}

fn started_running(before: &str) -> Arc<Session> {
    let session = with_agent_running(before);
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
    ui.wait_for("the form opens", |s| s.contains("New bot") && s.contains("For"));
    ui.send(b"prs");
    ui.click(ui.position_of("create"));
    ui.wait_for("its empty chat shows", |s| s.contains("Message prs…"));
    ui.send(b"which are blocked?\r");
    ui.wait_for("its answer is drawn", |s| s.contains("hello from the bot") && s.contains("cornercase status --json"));

    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.starts_with("you · ") && history.contains("which are blocked?"), "{history}");
}

const HEAD: &str = "a bot can only type into a pane that runs an agent";

#[test]
fn a_bot_cannot_type_into_a_shell_pane_but_you_can() {
    let session = started_running(
        "env -u CORNERCASE_BOT cornercase send --pane \"$(cat \"$0.pane\")\" --enter \"touch $0.typed\" > \"$0.out\" 2> \"$0.err\"\necho $? > \"$0.status\"",
    );
    let pane = held::new_shell(&session);
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
    let pane = held::new_shell(&session);

    let sent =
        session.command().env("CORNERCASE_BOT", "prs").args(["send", "--pane", &pane, "x"]).output().expect("run");

    assert!(refused(&sent, 1, HEAD), "{sent:?}");
}

#[test]
fn inside_a_bot_queue_add_schedules_for_it_and_a_project_entry_is_refused() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    let added = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .args(["queue", "add", "--every", "2h", "--prompt", "check my PRs", "--name", "Review PRs"])
        .output()
        .expect("run");
    assert!(added.status.success(), "{added:?}");
    let list: Value = serde_json::from_str(&session.says(&["queue", "list", "--bot", "prs", "--json"])).expect("json");
    assert_eq!(list["entries"][0]["place"]["bot"], "prs");
    assert_eq!(list["entries"][0]["repeat"], "2h");
    assert!(session.says(&["queue", "list", "--bot", "PRs"]).contains("Review PRs · bot prs"));
    let bots: Value = serde_json::from_str(&session.says(&["bot", "list", "--json"])).expect("json");
    assert_eq!((bots["bots"][0]["routines"].as_u64(), bots["bots"][0]["next"].is_i64()), (Some(1), true));

    let project = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .args(["queue", "add", "--project", "1", "--at", "in 2h", "--prompt", "x"])
        .output()
        .expect("run");
    assert!(refused(&project, 1, "a bot can only schedule and change its own queue entries"), "{project:?}");
    let mixed = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .args(["queue", "add", "--bot", "prs", "--project", "1", "--at", "in 2h", "--prompt", "x"])
        .output()
        .expect("run");
    assert_eq!(mixed.status.code(), Some(2), "{mixed:?}");
}

#[test]
fn a_bot_routine_run_now_answers_in_its_chat() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    let mut ui = Harness::open(Arc::clone(&session), ROWS, COLS);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));

    let id = session.says(&["queue", "add", "--bot", "prs", "--every", "2h", "--prompt", "check"]);
    let run = session.says(&["queue", "run", &id]);
    assert!(!run.is_empty(), "queue run prints the run's id");

    wait_until("the run answered in the chat", || {
        session.says(&["bot", "history", "prs"]).contains("hello from the bot")
    });
    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.contains("routine "), "{history}");
    let args = std::fs::read_to_string(session.dir.join("claude-bot.args")).expect("the agent ran");
    assert!(args.contains("This is your routine") && !args.contains("--resume"), "{args}");
    wait_until("its entry is answered", || {
        session.says(&["queue", "list", "--bot", "prs", "--json"]).contains("\"answered\"")
    });
}

#[test]
fn inside_a_bot_cornercase_writes_no_file_it_names_and_clones_nothing() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    let file = session.dir.join("settings.toml");
    let file_text = file.to_str().expect("UTF-8");

    let exported = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .args(["config", "export", "--to", file_text])
        .output()
        .expect("run");
    assert!(refused(&exported, 2, "a bot cannot have cornercase write to a file"), "{exported:?}");
    assert!(!file.exists());

    let mut command = session.cold_command();
    command.env("CORNERCASE_BOT", "prs").args(["requests", "--follow", "-"]);
    let mut typist = input::Typist::start(command);
    let answer = typist.types(&serde_json::json!({"args": ["config", "export", "--to", file_text]}));
    assert_eq!(answer["status"], 2, "{answer}");
    assert!(answer["error"].as_str().is_some_and(|e| e.contains("a bot cannot have cornercase write")), "{answer}");
    assert!(!file.exists());
    let _ = typist.ends();

    let origin = session.dir.join("repo");
    let target = session.dir.join("clones");
    let attempt = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .args(["clone", origin.to_str().expect("UTF-8"), target.to_str().expect("UTF-8")])
        .output()
        .expect("run");
    assert!(refused(&attempt, 1, "a bot cannot clone a repository"), "{attempt:?}");
    assert!(!target.exists());
}

#[test]
fn a_bots_turn_cannot_stop_restart_or_update_the_server_even_without_its_variable() {
    let stops = ["kill-server --yes", "restart --yes", "restart --when-idle --yes"];
    let before = stops
        .map(|words| format!("env -u CORNERCASE_BOT cornercase {words} >> \"$0.err\" 2>&1; echo $? >> \"$0.status\"\n"))
        .concat();
    let session = started_running(&before);
    let boot = session.says(&["contract", "--json"]);
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    session.says(&["bot", "say", "prs", "go", "--wait"]);

    let status = std::fs::read_to_string(session.dir.join("claude-bot.status")).expect("the commands ran");
    let stderr = std::fs::read_to_string(session.dir.join("claude-bot.err")).expect("their output");
    assert_eq!(status.lines().collect::<Vec<_>>(), ["1", "1", "1"], "{stderr}");
    assert_eq!(stderr.matches("a bot cannot stop its own server").count(), 3, "{stderr}");
    assert_eq!(session.says(&["contract", "--json"]), boot);
}
