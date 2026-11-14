use std::net::TcpStream;

use serde_json::{Value, json};
use tungstenite::client::IntoClientRequest as _;
use tungstenite::{Message, WebSocket};

use super::browser::http_get;
use super::commands::Followed;
use super::remote::gone;
use super::*;

type Socket = WebSocket<TcpStream>;

#[derive(Debug, Default)]
struct Seen {
    texts: Vec<String>,
    close: Option<u16>,
}

fn serving() -> (Arc<Session>, u16) {
    let session = Session::new();
    let port = TcpListener::bind(("127.0.0.1", 0)).expect("bind").local_addr().expect("address").port();
    std::fs::write(session.dir.join("config.json"), format!(r#"{{"phone_app": true, "phone_app_port": {port}}}"#))
        .expect("config");
    let mut command = session.cold_command();
    command.env("CORNERCASE_PHONE_ADDRESSES", "127.0.0.1").env("CORNERCASE_LOG", "debug").arg("start-server");
    let started = command.output().expect("run cornercase");
    assert!(started.status.success(), "{started:?}");
    (session, port)
}

fn connect(port: u16, path: &str, secret: Option<&str>) -> Result<Socket, u16> {
    let mut request = format!("ws://127.0.0.1:{port}{path}").into_client_request().expect("a request");
    if let Some(secret) = secret {
        request.headers_mut().insert("Authorization", format!("Bearer {secret}").parse().expect("a header"));
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.set_read_timeout(Some(TIMEOUT)).expect("read timeout");
    match tungstenite::client::client(request, stream) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))) => {
            Err(response.status().as_u16())
        }
        Err(e) => panic!("unexpected handshake failure: {e}"),
    }
}

fn read_all(ws: &mut Socket) -> Seen {
    let mut seen = Seen::default();
    while let Ok(message) = ws.read() {
        match message {
            Message::Text(text) => seen.texts.push(text.to_string()),
            Message::Close(frame) => seen.close = frame.map(|frame| u16::from(frame.code)),
            _ => {}
        }
    }
    seen
}

fn pairing(session: &Session, port: u16) -> (String, String) {
    let mut child = session.spawn(&["config", "pair", "app", "--json"]);
    let mut followed = Followed::of(&mut child);
    followed.until("the address", |line| line["url"].is_string());
    let url = followed.seen.iter().find_map(|line| line["url"].as_str()).expect("an address").to_string();
    assert!(url.starts_with(&format!("ws://127.0.0.1:{port}/?code=")), "{url}");
    let path = url.split_once(&format!(":{port}")).expect("the port").1.to_string();
    let mut ws = connect(port, &path, None).expect("paired");
    let seen = read_all(&mut ws);
    let reply: Value = serde_json::from_str(&seen.texts[0]).expect("json");
    assert_eq!(followed.next("the end"), json!({"paired": true}));
    let ended = finish_within(child, TIMEOUT);
    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    (reply["secret"].as_str().expect("a secret").to_string(), url)
}

fn paired(session: &Session, port: u16) -> String {
    pairing(session, port).0
}

fn run(port: u16, secret: &str, argv: &Value) -> Seen {
    let mut ws = connect(port, "/cli", Some(secret)).expect("an upgrade");
    ws.send(Message::text(json!({ "argv": argv }).to_string())).expect("argv");
    read_all(&mut ws)
}

fn events(session: &Session, port: u16, secret: &str) -> (Socket, String) {
    let mut ws = connect(port, "/cli", Some(secret)).expect("an upgrade");
    ws.send(Message::text(r#"{"argv":["events"]}"#)).expect("argv");
    (ws, started_pid(session))
}

fn pid_in(log: &str) -> String {
    log.lines()
        .rev()
        .find(|line| line.contains("phone app command started"))
        .and_then(|line| line.split("pid=").nth(1))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn started_pid(session: &Session) -> String {
    session.wait_for_file("server.log", "the command to start", |log| !pid_in(log).is_empty());
    pid_in(&std::fs::read_to_string(session.dir.join("server.log")).expect("the log"))
}

#[test]
fn a_paired_app_runs_contract_over_cli_and_the_log_holds_no_secret() {
    let (session, port) = serving();
    let (secret, url) = pairing(&session, port);
    let code = url.split("code=").nth(1).and_then(|rest| rest.split('&').next()).expect("a code").to_string();

    let seen = run(port, &secret, &json!(["contract", "--json"]));

    let stdout: String = seen
        .texts
        .iter()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter_map(|m| m["stdout"].as_str().map(String::from))
        .collect::<Vec<_>>()
        .join("\n");
    let contract: Value = serde_json::from_str(&stdout).expect("the contract");
    assert!(contract["features"].as_array().expect("features").iter().any(|f| f == "cli-endpoint"), "{contract}");
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
    assert_eq!(seen.close, Some(1000));
    let log = std::fs::read_to_string(session.dir.join("server.log")).expect("the log");
    let hex = secret.split_once('.').expect("id and secret").1;
    assert!(!log.contains(&code) && !log.contains(hex), "{log}");
    assert!(log.contains("phone app paired"), "{log}");
}

#[test]
fn cli_without_the_secret_is_403() {
    let (session, port) = serving();
    let _ = &session;

    let plain = http_get(port, "/cli");
    let upgrade = connect(port, "/cli", None).err();
    let wrong = connect(port, "/cli", Some(&format!("abcd.{}", "0".repeat(64)))).err();

    assert!(plain.starts_with("HTTP/1.1 403"), "{plain}");
    assert_eq!((upgrade, wrong), (Some(403), Some(403)));
}

#[rstest]
#[case::kill_server(json!(["kill-server"]))]
#[case::no_command(json!([]))]
#[case::start_server(json!(["start-server"]))]
#[case::logs(json!(["logs"]))]
#[case::plugin_list(json!(["plugin", "list"]))]
#[case::remote(json!(["status", "--remote", "vps"]))]
fn commands_that_are_not_for_the_server_are_refused(#[case] argv: Value) {
    let (session, port) = serving();
    let secret = paired(&session, port);

    let seen = run(port, &secret, &argv);

    assert_eq!(seen.texts.len(), 2, "{seen:?}");
    assert!(seen.texts[0].starts_with(r#"{"stderr":"#), "{seen:?}");
    assert_eq!(seen.texts[1], r#"{"exit":2}"#);
    assert!(session.says(&["status"]).len() > 1);
}

#[test]
fn closing_the_socket_kills_the_command() {
    let (session, port) = serving();
    let secret = paired(&session, port);

    let (ws, pid) = events(&session, port, &secret);
    drop(ws);

    wait_until("the command is gone", || gone(&pid));
}

#[test]
fn forgetting_the_app_disconnects_it() {
    let (session, port) = serving();
    let secret = paired(&session, port);
    let (id, _) = secret.split_once('.').expect("an id");

    let (mut ws, pid) = events(&session, port, &secret);
    let removed = session.cli(&["config", "remove", &format!("app.{id}")]);

    assert!(removed.status.success(), "{removed:?}");
    let seen = read_all(&mut ws);
    assert!(seen.texts.is_empty() || seen.close.is_none(), "{seen:?}");
    wait_until("the command is gone", || gone(&pid));
    assert_eq!(connect(port, "/cli", Some(&secret)).err(), Some(403));
}

#[test]
fn the_routes_are_where_it_listens() {
    let (session, port) = serving();

    let routes = session.says(&["config", "pair", "app", "--routes", "--json"]);
    let text = session.says(&["config", "pair", "app", "--routes"]);

    assert_eq!(
        serde_json::from_str::<Value>(&routes).expect("json"),
        json!({"routes": [{"host": "127.0.0.1", "port": port}]})
    );
    assert_eq!(text, format!("127.0.0.1:{port}"));
}

#[test]
fn kill_server_closes_running_commands_with_going_away() {
    let (session, port) = serving();
    let secret = paired(&session, port);

    let (mut ws, _) = events(&session, port, &secret);
    let killed = session.cli(&["kill-server"]);
    let seen = read_all(&mut ws);

    assert!(killed.status.success(), "{killed:?}");
    assert!(seen.texts.iter().all(|text| !text.starts_with(r#"{"exit""#)), "{seen:?}");
    assert_eq!(seen.close, Some(1001));
}

#[test]
fn requests_follow_runs_several_commands_over_one_socket() {
    let (session, port) = serving();
    let secret = paired(&session, port);

    let mut ws = connect(port, "/cli", Some(&secret)).expect("an upgrade");
    ws.send(Message::text(r#"{"argv":["requests","--follow","-"]}"#)).expect("argv");
    ws.send(Message::text(json!({"stdin": r#"{"args":["contract"]}"#}).to_string())).expect("a line");
    ws.send(Message::text(json!({"stdin": r#"{"args":["status","--json"]}"#}).to_string())).expect("a line");
    ws.send(Message::text(r#"{"eof":true}"#)).expect("eof");
    let seen = read_all(&mut ws);

    let answers: Vec<Value> = seen
        .texts
        .iter()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter_map(|message| message["stdout"].as_str().and_then(|line| serde_json::from_str(line).ok()))
        .collect();
    assert_eq!(answers.len(), 2, "{seen:?}");
    assert!(answers.iter().all(|answer| answer["status"] == 0), "{answers:?}");
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
}
