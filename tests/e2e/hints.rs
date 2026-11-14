use super::remote::{fake_ssh, ran, remote_says, remotely};
use super::*;

fn there(action: &str) -> String {
    format!("cornercase --remote devbox --remote-command {} {action}", bin())
}

#[test]
fn a_command_run_there_names_the_remote_in_its_hints() {
    let session = Session::new();

    let out = remotely(&session, &["update"]).output().expect("run cornercase");

    assert!(refused(&out, 1, &format!("`{}` only updates released builds", there("update"))), "{out:?}");
}

#[test]
fn a_plugin_command_there_names_the_remote_in_its_hints() {
    let session = Session::new();

    let said = remote_says(&session, &["plugin", "list"]);

    let hint = format!("no plugins; `{}`", there("plugin install <owner/repo>"));
    assert!(said.starts_with(&hint), "{said}");
}

#[test]
fn the_log_there_is_printed_as_it_is() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let path = session.says(&["logs", "--path"]);
    std::fs::write(&path, "see `cornercase status` now\n").expect("write the log");

    let said = remote_says(&session, &["logs"]);

    assert!(said.contains("see `cornercase status` now"), "{said}");
    assert!(!said.contains("--remote"), "{said}");
}

#[test]
fn a_question_there_shows_before_it_is_answered() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let path = fake_ssh(&session.dir, "");
    let args = ["--remote", "devbox", "--remote-command", bin(), "restart"];

    let mut app = Harness::command_in_pty(&args, Arc::clone(&session), &[("PATH", &path)]);

    app.wait_for("it asks", |s| s.contains("Restart it now? [y/N]"));
    app.send(b"n\r");
    app.wait_for("the server keeps running", |s| s.contains("the server keeps running"));
}

#[test]
fn a_hint_there_names_the_remote_in_a_terminal_too() {
    let session = Session::new();
    let path = fake_ssh(&session.dir, "");
    let args = ["--remote", "devbox", "--remote-command", bin(), "update"];

    let mut app = Harness::command_in_pty(&args, session, &[("PATH", &path)]);

    let hint = format!("`{}` only updates released builds", there("update"));
    app.wait_for_raw("the hint names the remote", |raw| raw.contains(&hint));
}

fn pane_there(session: &Session) -> String {
    remote_says(session, &["start-server"]);
    remote_says(session, &["new-tab"])
}

fn read_there(pane: &str) -> String {
    format!("`cornercase --remote devbox --remote-command {} read --pane {pane}` prints its screen", bin())
}

#[rstest]
#[case::by_the_flag(&["--remote", "devbox"], None)]
#[case::by_the_variable(&[], Some("devbox"))]
fn a_message_from_there_names_the_remote_in_the_command_it_gives(#[case] flag: &[&str], #[case] var: Option<&str>) {
    let session = Session::new();
    let pane = pane_there(&session);
    let mut args = flag.to_vec();
    args.extend(["--remote-command", bin(), "read", "--pane", &pane, "--last-message"]);

    let out = ran(&session, &args, var);

    assert!(refused(&out, 1, &read_there(&pane)), "{out:?}");
}

#[test]
fn each_entry_of_a_partial_answer_from_there_names_the_remote() {
    let session = Session::new();
    let first = pane_there(&session);
    let second = remote_says(&session, &["new-tab"]);

    let out = remotely(&session, &["read", "--pane", &first, "--pane", &second, "--last-message", "--json"])
        .output()
        .expect("run cornercase");

    let answer: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let errors: Vec<&str> =
        answer["panes"].as_array().expect("panes").iter().filter_map(|p| p["error"].as_str()).collect();
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(
        errors.len() == 2 && errors[0].contains(&read_there(&first)) && errors[1].contains(&read_there(&second)),
        "{answer}"
    );
}

#[test]
fn a_line_of_requests_that_fails_there_names_the_remote() {
    let session = Session::new();
    let pane = pane_there(&session);
    let mut typist = input::Typist::start(remotely(&session, &["requests", "--follow", "-"]));

    let answer = typist.types(&serde_json::json!({"args": ["read", "--pane", &pane, "--last-message"]}));

    assert_eq!(answer["status"], 1, "{answer}");
    assert!(answer["error"].as_str().is_some_and(|e| e.contains(&read_there(&pane))), "{answer}");
    assert_eq!(typist.ends().status.code(), Some(3));
}

#[test]
fn a_hint_this_command_writes_about_the_server_there_names_the_remote() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);

    let out = remotely(&session, &["config", "set", "no-such-setting", "1"]).output().expect("run cornercase");

    let hint = format!(
        "no setting `no-such-setting`; `cornercase --remote devbox --remote-command {} config` lists them",
        bin()
    );
    assert!(refused(&out, 2, &hint), "{out:?}");
}

#[test]
fn a_token_hint_this_command_writes_names_the_remote() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);

    let out = remotely(&session, &["config", "secret", "linear"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run cornercase");

    let hint = format!("| cornercase --remote devbox --remote-command {} config secret linear", bin());
    assert!(refused(&out, 2, &hint), "{out:?}");
}
