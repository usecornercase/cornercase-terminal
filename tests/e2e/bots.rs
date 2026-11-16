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

    assert_eq!(session.says(&["bot", "new", "PRs", "--agent", "claude"]), "prs");
    assert!(!session.dir.join("claude-bot.args").exists(), "making a bot without what it is for starts nothing");
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
fn a_bot_made_with_what_it_is_for_sets_itself_up_and_bot_setup_runs_it_again() {
    let session = started();

    assert_eq!(session.says(&["bot", "new", "PRs", "--agent", "claude", "--about", "review my PRs"]), "prs");
    assert_eq!(session.says(&["bot", "say", "prs", "and now?", "--wait"]), "hello from the bot");
    let args = std::fs::read_to_string(session.dir.join("claude-bot.args")).expect("the agent ran");
    assert!(args.contains("## You were just made") && args.contains("--resume s1"), "{args}");
    assert!(session.says(&["bot", "history", "prs"]).starts_with("— setting up · "));

    assert!(session.says(&["bot", "setup", "prs"]).parse::<u64>().is_ok());
    session.says(&["bot", "say", "prs", "again", "--wait"]);
    let args = std::fs::read_to_string(session.dir.join("claude-bot.args")).expect("the agent ran");
    assert_eq!(args.matches("## You were just made").count(), 2, "{args}");
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
fn inside_a_bot_cornercase_opens_no_window_and_relays_none() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    for args in [&[][..], &["proxy"], &["remote", "vps"]] {
        let opened = session.command().env("CORNERCASE_BOT", "prs").args(args).output().expect("run");
        assert!(refused(&opened, 2, "a bot cannot attach as a window"), "{args:?}: {opened:?}");
    }
    assert!(session.says(&["status", "--json"]).contains("\"projects\""));
}

#[test]
fn a_bot_made_from_the_form_answers_in_its_chat() {
    let session = started();
    let mut ui = Harness::open(Arc::clone(&session), ROWS, COLS);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));

    ui.pick_in_more("new bot…");
    ui.wait_for("the form opens", |s| s.contains("New bot") && s.contains("For") && s.contains("create"));
    ui.send(b"prs");
    ui.wait_for("the name is typed", |s| s.contains("prs"));
    ui.click(ui.position_of("create"));
    ui.wait_for("its empty chat shows", |s| s.contains("Message prs…"));
    ui.send(b"which are blocked?\r");
    ui.wait_for("its answer is drawn", |s| s.contains("hello from the bot") && s.contains("cornercase status --json"));

    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.starts_with("you · ") && history.contains("which are blocked?"), "{history}");
}

fn turn_ran(session: &Session) -> (String, String) {
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    session.says(&["bot", "say", "prs", "go", "--wait"]);
    let status = std::fs::read_to_string(session.dir.join("claude-bot.status")).expect("the commands ran");
    let stderr = std::fs::read_to_string(session.dir.join("claude-bot.err")).expect("their output");
    (status, stderr)
}

const HEAD: &str = "a bot can only type into a pane that runs an agent";

#[test]
fn a_bot_cannot_type_into_a_shell_pane_but_you_can() {
    let session = started_running(
        "env -u CORNERCASE_BOT cornercase send --pane \"$(cat \"$0.pane\")\" --enter \"touch $0.typed\" > \"$0.out\" 2> \"$0.err\"\necho $? > \"$0.status\"",
    );
    let pane = held::new_shell(&session);
    std::fs::write(session.dir.join("claude-bot.pane"), &pane).expect("the pane");
    let (status, stderr) = turn_ran(&session);
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
fn a_bots_routine_is_paused_from_its_routines_dialog() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    session.says(&["queue", "add", "--bot", "prs", "--every", "2h", "--prompt", "check", "--name", "Review PRs"]);
    let mut ui = Harness::open(Arc::clone(&session), ROWS, 160);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));

    ui.click(ui.position_of("prs"));
    ui.wait_for("its chat shows", |s| s.contains("Message prs…"));
    ui.click(ui.position_of("routines"));
    ui.wait_for("its routines dialog opens", |s| s.contains("Routines › prs") && s.contains("pause"));
    ui.click(ui.position_of("pause"));

    wait_until("the routine is paused", || {
        let list: Value =
            serde_json::from_str(&session.says(&["queue", "list", "--bot", "prs", "--json"])).expect("json");
        list["entries"][0]["paused"] == true
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
    let (status, stderr) = turn_ran(&session);
    assert_eq!(status.lines().collect::<Vec<_>>(), ["1", "1", "1"], "{stderr}");
    assert_eq!(stderr.matches("a bot cannot stop its own server").count(), 3, "{stderr}");
    assert_eq!(session.says(&["contract", "--json"]), boot);
}

#[test]
fn an_agent_a_bot_starts_is_tagged_and_its_chat_says_so() {
    let session = started_running(
        "[ -n \"$CORNERCASE_BOT\" ] || exit 0\ncornercase start claude --project \"$(cat \"$0.project\")\" --prompt fix > \"$0.start\" 2>&1",
    );
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("json");
    let project = status["projects"][0]["id"].as_u64().expect("a project").to_string();
    std::fs::write(session.dir.join("claude-bot.project"), &project).expect("the project");
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    let mut ui = Harness::open(Arc::clone(&session), ROWS, COLS);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));

    session.says(&["bot", "say", "prs", "go", "--wait"]);

    let started = std::fs::read_to_string(session.dir.join("claude-bot.start")).unwrap_or_default();
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("json");
    let panes: Vec<&Value> = status["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .flat_map(|project| project["workspaces"].as_array().into_iter().flatten())
        .flat_map(|workspace| workspace["tabs"].as_array().into_iter().flatten())
        .flat_map(|tab| tab["panes"].as_array().into_iter().flatten())
        .collect();
    assert_eq!(panes.iter().filter(|pane| pane["bot"].as_str() == Some("prs")).count(), 1, "{started}: {status}");
    let history = session.says(&["bot", "history", "prs"]);
    assert!(history.contains("▶ started claude in"), "{history}");
    let listed: Value = serde_json::from_str(&session.says(&["bot", "list", "--json"])).expect("json");
    assert_eq!(listed["bots"][0]["following"].as_array().map(Vec::len), Some(1), "{listed}");
}

#[test]
fn a_bots_turn_finds_its_servers_cornercase_first_and_your_gh_before_the_one_beside_it() {
    let session = with_agent_running("gh > \"$0.gh\" 2>&1\ncommand -v cornercase > \"$0.which\"");
    let server = installed_copy(&session, "shared");
    let shared = server.parent().expect("its folder").to_path_buf();
    write_executable(&shared.join("gh"), "#!/bin/sh\necho shared\n");
    let yours = session.folder("yours");
    write_executable(&yours.join("gh"), "#!/bin/sh\necho yours\n");
    let path = std::env::join_paths([yours.as_path(), shared.as_path(), "/usr/bin".as_ref(), "/bin".as_ref()])
        .expect("a path");
    let started = session.cold_command_of(&server).env("PATH", path).arg("start-server").output().expect("run");
    assert!(started.status.success(), "start-server failed: {started:?}");
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    session.says(&["bot", "say", "prs", "go", "--wait"]);

    let gh = std::fs::read_to_string(session.dir.join("claude-bot.gh")).expect("gh ran");
    assert_eq!(gh.trim(), "yours");
    let which = std::fs::read_to_string(session.dir.join("claude-bot.which")).expect("which ran");
    let found = PathBuf::from(which.trim());
    assert!(!found.starts_with(&shared), "{found:?}");
    assert_eq!(
        std::fs::canonicalize(&found).expect("it leads somewhere"),
        std::fs::canonicalize(&server).expect("copy")
    );
}
