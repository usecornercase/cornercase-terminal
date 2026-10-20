use serde_json::Value;

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
