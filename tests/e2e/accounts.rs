use serde_json::{Value, json};

use super::commands::Followed;
use super::*;

fn env_of(session: &Session, args: &[&str], file: &std::path::Path) -> String {
    let command = format!("env > {}.tmp && mv {0}.tmp {0}", file.display());
    let mut words: Vec<&str> = vec!["new-tab"];
    words.extend_from_slice(args);
    words.extend(["--", &command]);
    session.says(&words);
    wait_until("the tab writes its environment", || file.exists());
    std::fs::read_to_string(file).expect("the environment")
}

fn config_dir(env: &str) -> Option<&str> {
    env.lines().find_map(|line| line.strip_prefix("CLAUDE_CONFIG_DIR="))
}

#[test]
fn an_account_in_the_active_profile_reaches_new_tabs_and_another_profile_is_used_once() {
    let session = Session::new();
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");

    let added = session.says(&["account", "add", "claude", "work"]);
    assert!(added.starts_with("added the claude account work in "), "{added}");
    let listed: Value = serde_json::from_str(&session.says(&["account", "list", "--json"])).expect("the accounts");
    let path = listed["providers"][0]["accounts"][0]["path"].as_str().expect("its folder").to_string();
    assert!(std::path::Path::new(&path).is_dir(), "{path}");

    session.says(&["profile", "new", "Work", "claude=work"]);
    let used = session.says(&["profile", "use", "Work"]);
    assert!(used.contains("* Work  claude=work"), "{used}");

    let files = temp_dir("accounts");
    let inherited = env_of(&session, &[], &files.join("work.env"));
    assert_eq!(config_dir(&inherited), Some(path.as_str()));

    let system = env_of(&session, &["--profile", ""], &files.join("system.env"));
    assert_ne!(config_dir(&system), Some(path.as_str()));
    assert!(session.says(&["profile", "list"]).contains("* Work"), "the active profile stays");

    assert_unknown_profile_refused(&session);
    assert_account_removed(&session, &path);
    let _ = std::fs::remove_dir_all(files);
}

fn assert_unknown_profile_refused(session: &Session) {
    let refused = session.cli(&["start", "claude", "--profile", "Nope"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("there is no profile `Nope`"), "{refused:?}");
}

fn assert_account_removed(session: &Session, path: &str) {
    let removed = session.says(&["account", "remove", "claude", "work", "--yes"]);
    assert_eq!(removed, "removed the claude account work");
    assert!(!std::path::Path::new(path).exists());
    assert!(session.says(&["profile", "list"]).contains("* Work  the system's logins"));
}

#[test]
fn removing_an_account_without_a_terminal_needs_yes() {
    let session = Session::new();
    assert!(session.cold(&["start-server"]).status.success());
    session.says(&["account", "add", "claude", "work"]);

    let refused = session
        .command()
        .args(["account", "remove", "claude", "work"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run");

    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--yes"), "{refused:?}");
    assert!(session.says(&["account", "list"]).contains("  work  "));
}

fn accounts_revision(line: &Value) -> Option<u64> {
    line.get("accounts_revision").and_then(Value::as_u64)
}

fn following_a_new_server(session: &Session) -> (std::process::Child, Followed) {
    let mut start = session.cold_command();
    start.arg("start-server").env("GEMINI_CLI_HOME", session.dir.join("gemini-home"));
    let started = start.output().expect("run cornercase");
    assert!(started.status.success(), "start-server failed: {started:?}");
    let mut following = session.spawn(&["status", "--app", "--follow"]);
    let mut followed = Followed::of(&mut following);
    let first = followed.next("the first line");
    assert_eq!(accounts_revision(&first), None, "a server without accounts: {first}");
    (following, followed)
}

fn stop_following(session: &Session, mut following: std::process::Child) {
    session.says(&["kill-server"]);
    let status = following.wait().expect("the command ends");
    assert!(status.success(), "{status:?}");
}

fn config_revision(session: &Session) -> u64 {
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("the status");
    status["config"]["revision"].as_u64().expect("the config's revision")
}

#[test]
fn an_account_added_or_removed_by_a_command_moves_the_revision_of_the_app_stream() {
    let session = Session::new();
    let (following, mut followed) = following_a_new_server(&session);
    let config = config_revision(&session);

    session.says(&["account", "add", "gemini", "probe"]);
    followed.until("the line of the first account", |line| accounts_revision(line) == Some(1));
    let printed: Value = serde_json::from_str(&session.says(&["status", "--app"])).expect("one JSON document");
    session.says(&["account", "remove", "gemini", "probe", "--yes"]);
    followed.until("the line of its removal", |line| accounts_revision(line) == Some(2));

    assert_eq!(accounts_revision(&printed), Some(1), "{printed}");
    assert_eq!(config_revision(&session), config, "nothing else moved for a client to notice");
    stop_following(&session, following);
}

#[test]
fn the_first_folder_of_accounts_another_program_makes_moves_it_on_the_sync_step() {
    let session = Session::new();
    let (following, mut followed) = following_a_new_server(&session);

    std::fs::create_dir_all(session.dir.join("server.accounts").join("gemini").join("probe")).expect("its folder");

    followed.until("the line of the folder", |line| accounts_revision(line) == Some(1));
    assert!(session.says(&["account", "list"]).contains("probe"));
    stop_following(&session, following);
}

pub(super) fn fake_login(session: &Session) -> PathBuf {
    let out = session.dir.join("login-out");
    let fake = session.dir.join("claude");
    let script = format!(
        "#!/bin/sh\nprintf '%s|%s\\n' \"$*\" \"${{CLAUDE_CONFIG_DIR:-system}}\" > {0}.tmp && mv {0}.tmp {0}\n",
        out.display()
    );
    write_executable(&fake, &script);
    let config = json!({"agent_commands": agent_commands(&[("claude", &fake)])});
    std::fs::write(session.dir.join("config.json"), config.to_string()).expect("write the config");
    out
}

#[test]
fn an_account_logs_in_again_through_remote_and_one_that_cannot_is_refused() {
    let session = Session::new();
    let out = fake_login(&session);
    remote::remote_says(&session, &["start-server"]);
    let folder = temp_dir("login-again");
    remote::remote_says(&session, &["open", &folder.display().to_string()]);
    remote::remote_says(&session, &["account", "add", "claude", "work"]);
    let listed = remote::remote_says(&session, &["account", "list", "--json"]);
    let listed: Value = serde_json::from_str(&listed).expect("the accounts");
    let path = listed["providers"][0]["accounts"][0]["path"].as_str().expect("its folder").to_string();

    let answer = remote::remote_says(&session, &["account", "login", "claude", "work", "--json"]);
    let answer: Value = serde_json::from_str(&answer).expect("the answer");
    let unknown = remote::remotely(&session, &["account", "login", "claude", "nope"]).output().expect("run");
    let gemini = remote::remotely(&session, &["account", "login", "gemini", "--system"]).output().expect("run");

    assert!(answer["login"]["pane"].is_number() && answer["login"]["tab"].is_number(), "{answer}");
    wait_until("the login runs", || out.exists());
    assert_eq!(std::fs::read_to_string(&out).expect("the log").trim_end(), format!("auth login|{path}"));
    assert!(refused(&unknown, 1, "there is no claude account `nope`"), "{unknown:?}");
    assert!(refused(&gemini, 1, "no login command"), "{gemini:?}");
    let _ = std::fs::remove_dir_all(folder);
}
