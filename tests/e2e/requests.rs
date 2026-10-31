use serde_json::json;

use super::input::Typist;
use super::*;

fn requesting(session: &Session) -> Typist {
    let mut command = session.cold_command();
    command.args(["requests", "--follow", "-"]);
    Typist::start(command)
}

#[test]
fn requests_follow_runs_each_line_in_order_until_its_input_ends() {
    let session = Session::new();
    session.cold(&["start-server"]);
    let pane = session.says(&["new-tab"]);
    let mut typist = requesting(&session);

    let sent = typist.types(&json!({"args": ["send", "--pane", &pane, "-"], "stdin": "echo requests-\"\"one"}));
    assert_eq!(sent["status"], 0, "{sent}");
    let keyed = typist.types(&json!({"args": ["keys", "--pane", &pane, "enter"]}));
    assert_eq!(keyed["status"], 0, "{keyed}");
    let opened = typist.types(&json!({"args": ["new-tab", "--json"]}));
    assert_eq!(opened["status"], 0, "{opened}");
    assert!(opened["json"]["pane"].is_u64(), "{opened}");
    let plain = typist.types(&json!({"args": ["new-tab"]}));
    assert!(plain["text"].as_str().is_some_and(|text| text.trim().parse::<u64>().is_ok()), "{plain}");

    session.says(&["wait", "--pane", &pane, "--text", "^requests-one$", "--timeout", "10"]);
    let ended = typist.ends();
    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
}

fn answered(typist: &mut Typist, line: &serde_json::Value, status: u64) -> serde_json::Value {
    let answer = typist.types(line);
    assert_eq!(answer["status"], status, "{line}: {answer}");
    answer
}

#[test]
fn requests_follow_refuses_a_line_and_runs_the_next() {
    let session = Session::new();
    session.cold(&["start-server"]);
    let mut typist = requesting(&session);

    let events = answered(&mut typist, &json!({"args": ["events"]}), 2);
    assert!(events["error"].as_str().is_some_and(|e| e.contains("connection of its own")), "{events}");
    answered(&mut typist, &json!({"args": ["kill-server"]}), 2);
    session.says(&["status"]);
    answered(&mut typist, &json!({"args": ["focus", "--bogus"]}), 2);
    answered(&mut typist, &json!({"stdni": "x"}), 2);
    let missing = answered(&mut typist, &json!({"args": ["close", "--pane", "99999"]}), 1);
    assert!(missing["error"].is_string(), "{missing}");
    let status = answered(&mut typist, &json!({"args": ["status", "--json"]}), 0);
    assert!(status["json"]["boot"].is_string(), "{status}");

    let ended = typist.ends();
    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(3), &b""[..]), "{ended:?}");
}

#[test]
fn a_waiting_line_ends_when_its_reader_leaves() {
    let session = Session::new();
    session.cold(&["start-server"]);
    let pane = session.says(&["new-tab"]);
    let mut requests = session
        .cold_command()
        .args(["requests", "--follow", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run cornercase");
    let mut input = requests.stdin.take().expect("its input");
    let mut out = std::io::BufReader::new(requests.stdout.take().expect("stdout"));
    writeln!(input, "{}", json!({"args": ["status", "--json"]})).expect("a line");
    std::io::BufRead::read_line(&mut out, &mut String::new()).expect("the first answer");
    let waiting = json!({"args": ["wait", "--pane", &pane, "--text", "never-shown", "--timeout", "120"]});
    writeln!(input, "{waiting}").expect("a line");
    input.flush().expect("flush");

    drop(out);
    let ended = finish_within(requests, TIMEOUT);

    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
    drop(input);
}
