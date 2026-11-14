use super::remote::{fake_ssh, remote_says, remotely};
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
