use std::thread;

use serde_json::{Value, json};

use super::parse;
use crate::cli::line::{self, Printed};
use crate::cli::{Command, Control, run_mark, run_send};
use crate::client;
use crate::control::Response;
use crate::os::socket::UnixStream;
use crate::protocol::{self, ClientMessage, ServerMessage};

const WARNING: &str =
    "stage 3 (tester) has not run since the last change in stage 2: run it first, or say in your summary why not";

fn ran(words: &[&str], answer: Value) -> Printed {
    let (ours, mut theirs) = UnixStream::pair().expect("a socket pair");
    let server = thread::spawn(move || {
        while let Ok(Some(ClientMessage::Request(_))) = protocol::recv(&mut theirs) {
            let reply = serde_json::to_string(&Response::Ok(answer.clone())).expect("json");
            protocol::send(&mut theirs, &ServerMessage::Response(reply)).expect("an answer");
        }
    });
    client::share_socket(ours);
    let cli = parse(words).expect("the arguments");
    let (result, printed) = line::run(None, || match cli.command {
        Some(Command::Control(Control::Send(send))) => run_send(send).map(|()| true),
        Some(Command::Control(Control::Mark(mark))) => run_mark(&mark).map(|()| true),
        _ => panic!("not send or mark"),
    });
    client::unshare();
    server.join().expect("the fake server");
    result.expect("the command");
    printed
}

#[test]
fn a_send_prints_the_warning_on_stderr_and_nothing_on_stdout() {
    let printed = ran(&["send", "--pane", "3", "hi"], json!({"pane": 3, "warning": WARNING}));

    assert_eq!((printed.text.as_str(), printed.stderr), ("", format!("warning: {WARNING}\n")));
}

#[test]
fn a_send_that_waits_keeps_stdout_as_the_ending() {
    let printed = ran(
        &["send", "--pane", "3", "--enter", "--wait", "hi"],
        json!({"pane": 3, "ended": "idle", "warning": WARNING}),
    );

    assert_eq!((printed.text.as_str(), printed.stderr), ("idle\n", format!("warning: {WARNING}\n")));
}

#[test]
fn a_json_send_holds_the_warning_in_its_document_alone() {
    let printed = ran(&["send", "--pane", "3", "--json", "hi"], json!({"pane": 3, "warning": WARNING}));

    assert_eq!(printed.json.expect("a document")["warning"], WARNING);
    assert_eq!(printed.stderr, "");
}

#[test]
fn a_mark_prints_the_warning_on_stderr() {
    let printed = ran(&["mark", "--pane", "3"], json!({"pane": 3, "warning": WARNING}));

    assert_eq!((printed.text.as_str(), printed.stderr), ("", format!("warning: {WARNING}\n")));
}

#[test]
fn an_answer_without_a_warning_prints_none() {
    let printed = ran(&["send", "--pane", "3", "hi"], json!({"pane": 3}));

    assert_eq!((printed.text.as_str(), printed.stderr.as_str()), ("", ""));
}
