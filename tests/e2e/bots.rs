use serde_json::Value;

use super::commands::Followed;
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

fn bot_state(session: &Session) -> Value {
    let list: Value = serde_json::from_str(&session.says(&["bot", "list", "--json"])).expect("json");
    list["bots"][0].clone()
}

fn answered(session: &Session) -> Value {
    session.says(&["bot", "say", "prs", "hi"]);
    wait_until("the bot answered", || {
        let bot = bot_state(session);
        bot["state"] == "idle" && bot["last"]["kind"] == "bot"
    });
    bot_state(session)
}

#[test]
fn a_view_that_shows_a_bot_keeps_its_answers_read() {
    let session = started();
    session.says(&["bot", "new", "PRs", "--agent", "claude"]);
    let mut view = session.spawn(&["view", "--bot", "prs"]);
    let mut followed = Followed::of(&mut view);
    followed.until("the first frame", |frame| frame["revision"] == 1);
    assert_eq!(followed.seen[0]["view"]["bot"], "prs");
    let id = followed.seen[0]["view"]["id"].as_u64().expect("a view id").to_string();

    let shown = answered(&session);
    let cleared = session.says(&["view-set", "--view", &id, "--bot", ""]);
    let told = answered(&session);
    let _ = view.kill();
    let _ = view.wait();

    assert_eq!(shown["unread"], 0);
    assert!(!cleared.ends_with("bot prs"), "{cleared}");
    assert_eq!(told["unread"], 1);
}

#[test]
fn a_bot_made_with_what_it_is_for_sets_itself_up_and_bot_setup_runs_it_again() {
    let session = started();

    assert_eq!(session.says(&["bot", "new", "PRs", "--agent", "claude", "--about", "review my PRs"]), "prs");
    assert_eq!(session.says(&["bot", "say", "prs", "and now?", "--wait"]), "hello from the bot");
    let args = std::fs::read_to_string(session.dir.join("claude-bot.args")).expect("the agent ran");
    assert!(args.contains("## You were just made") && args.contains("--resume s1"), "{args}");
    assert!(session.says(&["bot", "history", "prs"]).starts_with("— setting up · "));

    assert_eq!(String::from_utf8_lossy(&session.cli(&["bot", "about", "prs"]).stdout), "review my PRs\n");
    let about: Value = serde_json::from_str(&session.says(&["bot", "about", "prs", "--json"])).expect("json");
    assert_eq!(about, serde_json::json!({"bot": "prs", "about": "review my PRs"}));

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
    let elsewhere = "a bot works on this computer only: it cannot act on another machine";
    let flag =
        session.command().env("CORNERCASE_BOT", "prs").args(["--remote", "vps", "status"]).output().expect("run");
    assert!(refused(&flag, 2, elsewhere), "{flag:?}");
    let named = session
        .command()
        .env("CORNERCASE_BOT", "prs")
        .env("CORNERCASE_REMOTE", "vps")
        .arg("status")
        .output()
        .expect("run");
    assert!(refused(&named, 2, elsewhere), "{named:?}");
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
fn a_bots_keys_and_closing_a_project_are_refused_by_the_server_but_yours_are_not() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    let pane = held::new_shell(&session);
    let as_bot = |args: &[&str]| session.command().env("CORNERCASE_BOT", "prs").args(args).output().expect("run");

    let keys = as_bot(&["keys", "--pane", &pane, "enter"]);
    let close = as_bot(&["close", "--project", "1"]);

    assert!(refused(&keys, 1, "a bot cannot press keys"), "{keys:?}");
    assert!(refused(&close, 1, "a bot closes only the tabs and panes it started"), "{close:?}");
    session.says(&["keys", "--pane", &pane, "enter"]);
    assert!(session.says(&["read", "--pane", &pane]).contains('$'));
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

fn a_bot_with_a_routine_shown() -> (Arc<Session>, Harness) {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);
    session.says(&["queue", "add", "--bot", "prs", "--every", "2h", "--prompt", "check", "--name", "Review PRs"]);
    let mut ui = Harness::open(Arc::clone(&session), ROWS * 2, 160);
    ui.wait_for("the window is drawn", |s| s.contains(ui::MORE_LABEL));
    ui.click(ui.position_of("prs"));
    ui.wait_for("its chat shows", |s| s.contains("Message prs…"));
    (session, ui)
}

fn wait_paused(session: &Session) {
    wait_until("the routine is paused", || {
        let list: Value =
            serde_json::from_str(&session.says(&["queue", "list", "--bot", "prs", "--json"])).expect("json");
        list["entries"][0]["paused"] == true
    });
}

#[test]
fn a_bots_routine_is_paused_from_its_routines_dialog() {
    let (session, mut ui) = a_bot_with_a_routine_shown();
    ui.click(ui.position_of("routines"));
    ui.wait_for("its routines dialog opens", |s| s.contains("Routines › prs") && s.contains("pause"));
    ui.click(ui.position_of("pause"));
    wait_paused(&session);
}

#[test]
fn a_bots_routine_is_paused_from_the_menu_on_its_card() {
    let (session, mut ui) = a_bot_with_a_routine_shown();
    ui.wait_for("its routine is on the card", |s| s.contains("Review PRs"));
    ui.right_click(ui.position_of("Review PRs"));
    ui.wait_for("its menu opens", |s| s.contains("run now") && s.contains("edit…"));
    ui.click(ui.position_of("pause"));
    wait_paused(&session);
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

const VALUE: &str = "hunter2-marker-value";
const READ: &str = "cornercase bot secret get pw > \"$0.$CORNERCASE_BOT.got\" 2>&1\necho $? > \"$0.$CORNERCASE_BOT.status\"\nenv -u CORNERCASE_BOT cornercase bot secret list a > \"$0.$CORNERCASE_BOT.list\" 2>&1";

fn read_by(session: &Session, bot: &str, what: &str) -> String {
    std::fs::read_to_string(session.dir.join(format!("claude-bot.{bot}.{what}"))).expect("what the turn wrote")
}

fn saved(session: &Session, bot: &str) {
    let out = session.fed(&["bot", "secret", "set", bot, "pw"], &format!("{VALUE}\n"));
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "pw");
}

fn counted(session: &Session) -> u64 {
    let json: Value = serde_json::from_str(&session.says(&["bot", "list", "--json"])).expect("json");
    json["bots"][0]["secrets"].as_u64().expect("a count")
}

#[test]
fn a_secret_is_read_back_by_its_bots_own_turn_and_by_nothing_else() {
    let session = started_running(READ);
    for args in [&["bot", "new", "a", "--agent", "claude"][..], &["bot", "new", "b", "--agent", "claude"]] {
        session.says(args);
    }
    saved(&session, "a");

    let listed = session.says(&["bot", "secret", "list", "a"]);
    assert!(listed.starts_with("pw\tsaved ") && listed.ends_with("key: file"), "{listed}");
    assert!(!listed.contains(VALUE));
    assert_eq!(counted(&session), 1);

    session.says(&["bot", "say", "a", "go", "--wait"]);
    assert_eq!(read_by(&session, "a", "got"), format!("{VALUE}\n"));
    assert_eq!(read_by(&session, "a", "status").trim(), "0");

    let plain = session.cli(&["bot", "secret", "get", "pw"]);
    assert!(refused(&plain, 1, "only the bot itself reads its secrets"), "{plain:?}");

    another_bots_turn_gets_nothing(&session);
}

fn another_bots_turn_gets_nothing(session: &Session) {
    session.says(&["bot", "say", "b", "go", "--wait"]);
    assert!(read_by(session, "b", "got").contains("no secret pw in b"), "{}", read_by(session, "b", "got"));
    assert_eq!(read_by(session, "b", "status").trim(), "1");
    assert!(read_by(session, "b", "list").contains("a bot reads only its own secrets"));
}

#[test]
fn a_full_access_bot_reads_its_own_secret_too() {
    let session = started_running(READ);
    session.says(&["bot", "new", "a", "--agent", "claude", "--access", "full"]);
    saved(&session, "a");

    session.says(&["bot", "say", "a", "go", "--wait"]);

    assert_eq!(read_by(&session, "a", "got"), format!("{VALUE}\n"));
}

#[test]
fn a_bot_cannot_save_or_remove_secrets_and_wrong_usage_is_refused() {
    let session = started();
    session.says(&["bot", "new", "prs", "--agent", "claude"]);

    for args in [&["bot", "secret", "set", "prs", "pw"][..], &["bot", "secret", "remove", "prs", "--all"]] {
        let out = session.command().env("CORNERCASE_BOT", "prs").args(args).output().expect("run");
        assert!(refused(&out, 2, "a bot cannot save or remove secrets"), "{args:?}: {out:?}");
    }
    let other = session.command().env("CORNERCASE_BOT", "prs").args(["bot", "secret", "list", "other"]).output();
    assert!(refused(&other.expect("run"), 2, "a bot reads only its own secrets"));
    let value_as_argument = session.cli(&["bot", "secret", "set", "prs", "pw", VALUE]);
    assert_eq!(value_as_argument.status.code(), Some(2), "{value_as_argument:?}");
    let empty = session.fed(&["bot", "secret", "set", "prs", "pw"], "\n");
    assert!(String::from_utf8_lossy(&empty.stderr).contains("the value is empty"), "{empty:?}");
    let big = session.fed(&["bot", "secret", "set", "prs", "pw"], &"a".repeat(8193));
    assert!(String::from_utf8_lossy(&big.stderr).contains("8193"), "{big:?}");
}

#[test]
fn the_value_is_in_no_file_and_no_log_line_and_one_line_break_is_dropped() {
    let session = with_agent_running(READ);
    let started = session.cold_command().env("CORNERCASE_LOG", "debug").arg("start-server").output().expect("run");
    assert!(started.status.success(), "{started:?}");
    session.says(&["bot", "new", "a", "--agent", "claude"]);
    let out = session.fed(&["bot", "secret", "set", "a", "pw"], &format!("{VALUE}\n\n"));
    assert!(out.status.success(), "{out:?}");
    session.says(&["bot", "say", "a", "go", "--wait"]);
    assert_eq!(read_by(&session, "a", "got"), format!("{VALUE}\n\n"));
    session.says(&["bot", "secret", "list", "a", "--json"]);
    session.says(&["bot", "secret", "remove", "a", "pw"]);

    let log = session.says(&["logs", "-n", "1000"]);
    assert!(log.contains("name=pw") && log.contains("bytes=21"), "{log}");
    assert!(!log.contains(VALUE), "{log}");
    saved(&session, "a");
    let found = std::process::Command::new("grep")
        .args(["-rl", VALUE])
        .arg(session.dir.join("server.bots"))
        .output()
        .expect("grep");
    assert!(found.stdout.is_empty(), "{found:?}");
    assert!(session.dir.join("server.bots").join("a").join("secrets").exists());
}

#[test]
fn removing_a_bot_removes_its_secrets_and_remove_all_starts_again() {
    let session = started();
    session.says(&["bot", "new", "a", "--agent", "claude"]);
    saved(&session, "a");
    let file = session.dir.join("server.bots").join("a").join("secrets");
    assert!(file.exists());
    session.says(&["bot", "secret", "remove", "a", "--all"]);
    assert!(!file.exists());
    assert!(session.says(&["bot", "secret", "list", "a"]).contains("a has no secrets"));
    saved(&session, "a");
    session.says(&["bot", "remove", "a"]);
    assert!(!file.exists());
}

const ASK: &str = "cornercase bot secret ask campus-user campus-password --about 'to log in' >> \"$0.$CORNERCASE_BOT.ask\" 2>&1\necho \"status $?\" >> \"$0.$CORNERCASE_BOT.ask\"\ncornercase bot secret get campus-password >> \"$0.$CORNERCASE_BOT.got\" 2>&1";

fn asked_ask(session: &Session) -> String {
    let state: Value = serde_json::from_str(&session.says(&["status", "--app"])).expect("one JSON document");
    let asks = &state["bots"][0]["asks"];
    assert_eq!(asks[0]["names"], serde_json::json!(["campus-user", "campus-password"]), "{state}");
    asks[0]["id"].as_u64().expect("an ask id").to_string()
}

#[test]
fn a_bot_asks_for_a_secret_the_user_gives_it_and_its_next_turn_reads_it() {
    let session = with_agent_running(ASK);
    let started = session.cold_command().env("CORNERCASE_LOG", "debug").arg("start-server").output().expect("run");
    assert!(started.status.success(), "{started:?}");
    let events = session.spawn(&["events"]);
    session.says(&["bot", "new", "a", "--agent", "claude"]);
    session.says(&["bot", "say", "a", "go", "--wait"]);
    assert!(
        read_by(&session, "a", "ask").starts_with("asked: campus-user, campus-password (ask "),
        "{}",
        read_by(&session, "a", "ask")
    );
    let history: Value = serde_json::from_str(&session.says(&["bot", "history", "a", "--json"])).expect("json");
    let lines = history["lines"].as_array().expect("lines");
    assert!(lines.iter().any(|line| line["kind"] == "notice" && line["ask"].is_u64()), "{history}");
    let ask = asked_ask(&session);

    let given = session.fed(
        &["bot", "secret", "give", "a", &ask],
        &format!("{{\"campus-user\":\"me\",\"campus-password\":\"{VALUE}\"}}"),
    );
    assert!(given.status.success(), "{given:?}");
    assert_eq!(String::from_utf8_lossy(&given.stdout), "campus-password\ncampus-user\n");
    wait_until("the turn cornercase started read the value", || {
        std::fs::read_to_string(session.dir.join("claude-bot.a.got")).is_ok_and(|got| got.contains(VALUE))
    });
    wait_until("that turn ended", || bot_state(&session)["state"] == "idle");
    assert!(session.says(&["bot", "history", "a"]).contains("secret saved: campus-user, campus-password"));
    let again = read_by(&session, "a", "ask");
    assert!(again.contains("campus-user is already saved") && again.ends_with("status 1\n"), "{again}");

    let mut events = events;
    let _ = events.kill();
    let streamed = String::from_utf8_lossy(&events.wait_with_output().expect("the events").stdout).to_string();
    let chat = std::fs::read_to_string(session.dir.join("server.bots").join("a").join("chat.jsonl")).expect("chat");
    for text in [chat, session.says(&["logs", "-n", "1000"]), session.says(&["status", "--app"]), streamed] {
        assert!(!text.contains(VALUE), "{text}");
    }
}

#[test]
fn only_the_bot_asks_and_only_the_user_gives_or_declines() {
    let session = started_running(ASK);
    session.says(&["bot", "new", "a", "--agent", "claude"]);
    let mine = session.cli(&["bot", "secret", "ask", "pw", "--about", "to log in"]);
    assert!(refused(&mine, 1, "only the bot itself asks for a secret"), "{mine:?}");
    session.says(&["bot", "say", "a", "go", "--wait"]);
    let ask = asked_ask(&session);
    let by_bot = session.command().env("CORNERCASE_BOT", "a").args(["bot", "secret", "decline", "a", &ask]).output();
    assert!(refused(&by_bot.expect("run"), 2, "a bot cannot give or decline what a bot asks for"));

    session.says(&["bot", "secret", "decline", "a", &ask]);

    assert!(session.says(&["bot", "history", "a"]).contains("secret not given: campus-user, campus-password"));
    let state: Value = serde_json::from_str(&session.says(&["status", "--app"])).expect("json");
    assert!(state["bots"][0].get("asks").is_none(), "{state}");
}
