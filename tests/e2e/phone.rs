use std::net::TcpStream;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use rustls::{ClientConfig, RootCertStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tungstenite::client::IntoClientRequest as _;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Connector, Message, WebSocket};

use super::browser::http_get;
use super::commands::Followed;
use super::remote::gone;
use super::*;

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

const CERTIFICATE: &str = "app-certificate.json";

struct Listener {
    port: u16,
    trust: Arc<ClientConfig>,
}

#[derive(Debug, Default)]
struct Seen {
    texts: Vec<String>,
    close: Option<u16>,
    timed_out: bool,
}

fn start(session: &Session) {
    let mut command = session.cold_command();
    command.env("CORNERCASE_PHONE_ADDRESSES", "127.0.0.1").env("CORNERCASE_LOG", "debug").arg("start-server");
    let started = command.output().expect("run cornercase");
    assert!(started.status.success(), "{started:?}");
    session.wait_for_file(CERTIFICATE, "the phone app's certificate", |text| text.contains(r#""key""#));
}

fn serving() -> (Arc<Session>, Listener) {
    let session = Session::new();
    let port = TcpListener::bind(("127.0.0.1", 0)).expect("bind").local_addr().expect("address").port();
    std::fs::write(session.dir.join("config.json"), format!(r#"{{"phone_app": true, "phone_app_port": {port}}}"#))
        .expect("config");
    start(&session);
    let trust = trusting(&certificate(&session));
    (session, Listener { port, trust })
}

fn saved_certificate(session: &Session) -> Value {
    let text = std::fs::read_to_string(session.dir.join(CERTIFICATE)).expect("the certificate");
    serde_json::from_str(&text).expect("json")
}

fn certificate(session: &Session) -> Vec<u8> {
    let saved = saved_certificate(session);
    STANDARD.decode(saved["certificate"].as_str().expect("a certificate")).expect("base64")
}

fn fingerprint_of(der: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(der))
}

fn fingerprint_in(url: &str) -> &str {
    url.split("&fp=").nth(1).expect("a fingerprint")
}

fn trusting(der: &[u8]) -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.add(der.to_vec().into()).expect("the certificate as the only root");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

fn connect(listener: &Listener, path: &str, secret: Option<&str>) -> Result<Socket, u16> {
    let url = format!("wss://cornercase:{}{path}", listener.port);
    handshake(listener.port, &url, secret, Connector::Rustls(Arc::clone(&listener.trust)))
}

fn connect_plain(port: u16, path: &str, secret: Option<&str>) -> Result<Socket, u16> {
    handshake(port, &format!("ws://127.0.0.1:{port}{path}"), secret, Connector::Plain)
}

fn handshake(port: u16, url: &str, secret: Option<&str>, connector: Connector) -> Result<Socket, u16> {
    let mut request = url.into_client_request().expect("a request");
    if let Some(secret) = secret {
        request.headers_mut().insert("Authorization", format!("Bearer {secret}").parse().expect("a header"));
    }
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.set_read_timeout(Some(TIMEOUT)).expect("read timeout");
    match tungstenite::client_tls_with_config(request, stream, None, Some(connector)) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))) => {
            Err(response.status().as_u16())
        }
        Err(_) => Err(0),
    }
}

fn read_all(ws: &mut Socket) -> Seen {
    let mut seen = Seen::default();
    loop {
        match ws.read() {
            Ok(Message::Text(text)) => seen.texts.push(text.to_string()),
            Ok(Message::Close(frame)) => seen.close = frame.map(|frame| u16::from(frame.code)),
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
            {
                seen.timed_out = true;
                return seen;
            }
            Err(_) => return seen,
        }
    }
}

struct Paired {
    secret: String,
    url: String,
    ended: std::process::Output,
}

fn pairing_with(session: &Session, args: &[&str], connect: impl Fn(&str) -> Result<Socket, u16>) -> Paired {
    let mut child = session.spawn(args);
    let mut followed = Followed::of(&mut child);
    followed.until("the address", |line| line["url"].is_string());
    let url = followed.seen.iter().find_map(|line| line["url"].as_str()).expect("an address").to_string();
    let path = url.split_once("://").and_then(|(_, rest)| rest.find('/').map(|at| rest[at..].to_string()));
    let mut ws = connect(&path.expect("a path")).expect("paired");
    let seen = read_all(&mut ws);
    let reply: Value = serde_json::from_str(&seen.texts[0]).expect("json");
    assert_eq!(followed.next("the end"), json!({"paired": true}));
    let ended = finish_within(child, TIMEOUT);
    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    Paired { secret: reply["secret"].as_str().expect("a secret").to_string(), url, ended }
}

fn pairing(session: &Session, listener: &Listener) -> (String, String) {
    let paired = pairing_with(session, &["config", "pair", "app", "--json"], |path| connect(listener, path, None));
    assert!(paired.url.starts_with(&format!("wss://127.0.0.1:{}/?code=", listener.port)), "{}", paired.url);
    assert_eq!(fingerprint_in(&paired.url), fingerprint_of(&certificate(session)));
    (paired.secret, paired.url)
}

fn paired(session: &Session, listener: &Listener) -> String {
    pairing(session, listener).0
}

fn run_on(mut ws: Socket, argv: &Value) -> Seen {
    ws.send(Message::text(json!({ "argv": argv }).to_string())).expect("argv");
    read_all(&mut ws)
}

fn run(listener: &Listener, secret: &str, argv: &Value) -> Seen {
    run_on(connect(listener, "/cli", Some(secret)).expect("an upgrade"), argv)
}

fn stdout_of(seen: &Seen) -> String {
    seen.texts
        .iter()
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .filter_map(|m| m["stdout"].as_str().map(String::from))
        .collect::<Vec<_>>()
        .join("\n")
}

fn events(session: &Session, listener: &Listener, secret: &str) -> (Socket, String) {
    let mut ws = connect(listener, "/cli", Some(secret)).expect("an upgrade");
    ws.send(Message::text(r#"{"argv":["events"]}"#)).expect("argv");
    let pid = started_pid(session);
    assert!(!gone(&pid), "the command {pid:?} is not running");
    (ws, pid)
}

fn pid_in(log: &str) -> String {
    log.lines()
        .rev()
        .find(|line| line.contains("phone app command started"))
        .and_then(|line| line.split("pid=").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .filter(|pid| pid.parse::<u32>().is_ok())
        .unwrap_or_default()
        .to_string()
}

fn started_pid(session: &Session) -> String {
    session.wait_for_file("server.log", "the command to start", |log| !pid_in(log).is_empty());
    pid_in(&std::fs::read_to_string(session.dir.join("server.log")).expect("the log"))
}

#[test]
fn a_paired_app_runs_contract_over_cli_and_the_log_holds_no_secret() {
    let (session, listener) = serving();
    let (secret, url) = pairing(&session, &listener);
    let code = url.split("code=").nth(1).and_then(|rest| rest.split('&').next()).expect("a code").to_string();

    let seen = run(&listener, &secret, &json!(["contract", "--json"]));

    let contract: Value = serde_json::from_str(&stdout_of(&seen)).expect("the contract");
    let features = contract["features"].as_array().expect("features");
    assert!(["cli-endpoint", "cli-tls"].iter().all(|f| features.iter().any(|g| g == f)), "{contract}");
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
    assert_eq!(seen.close, Some(1000));
    let log = std::fs::read_to_string(session.dir.join("server.log")).expect("the log");
    let hex = secret.split_once('.').expect("id and secret").1;
    let key = saved_certificate(&session)["key"].as_str().expect("a key").to_string();
    assert!(!log.contains(&code) && !log.contains(hex) && !log.contains(&key), "{log}");
    assert!(log.contains(fingerprint_in(&url)), "{log}");
    assert!(log.contains("phone app paired") && log.contains("tls=true"), "{log}");
}

#[test]
fn a_plain_client_still_pairs_and_runs_commands() {
    let (session, listener) = serving();
    let port = listener.port;

    let paired = pairing_with(&session, &["config", "pair", "app", "--json"], |path| connect_plain(port, path, None));
    let seen = run_on(connect_plain(port, "/cli", Some(&paired.secret)).expect("an upgrade"), &json!(["contract"]));

    assert!(stdout_of(&seen).contains("cli-endpoint"), "{seen:?}");
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
    let log = std::fs::read_to_string(session.dir.join("server.log")).expect("the log");
    assert!(log.contains("tls=false"), "{log}");
}

#[test]
fn cli_without_the_secret_is_403() {
    let (session, listener) = serving();
    let _ = &session;

    let plain = http_get(listener.port, "/cli");
    let upgrade = connect(&listener, "/cli", None).err();
    let wrong = connect(&listener, "/cli", Some(&format!("abcd.{}", "0".repeat(64)))).err();

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
    let (session, listener) = serving();
    let secret = paired(&session, &listener);

    let seen = run(&listener, &secret, &argv);

    assert_eq!(seen.texts.len(), 2, "{seen:?}");
    assert!(seen.texts[0].starts_with(r#"{"stderr":"#), "{seen:?}");
    assert_eq!(seen.texts[1], r#"{"exit":2}"#);
    assert!(session.says(&["status"]).len() > 1);
}

#[test]
fn closing_the_socket_kills_the_command() {
    let (session, listener) = serving();
    let secret = paired(&session, &listener);

    let (ws, pid) = events(&session, &listener, &secret);
    drop(ws);

    wait_until("the command is gone", || gone(&pid));
}

#[test]
fn forgetting_the_app_disconnects_it() {
    let (session, listener) = serving();
    let secret = paired(&session, &listener);
    let (id, _) = secret.split_once('.').expect("an id");

    let (mut ws, pid) = events(&session, &listener, &secret);
    let removed = session.cli(&["config", "remove", &format!("app.{id}")]);

    assert!(removed.status.success(), "{removed:?}");
    let seen = read_all(&mut ws);
    assert!(!seen.timed_out && seen.texts.is_empty() && seen.close.is_none(), "{seen:?}");
    wait_until("the command is gone", || gone(&pid));
    assert_eq!(connect(&listener, "/cli", Some(&secret)).err(), Some(403));
}

#[test]
fn the_routes_are_where_it_listens() {
    let (session, listener) = serving();
    let port = listener.port;

    let routes = session.says(&["config", "pair", "app", "--routes", "--json"]);
    let text = session.says(&["config", "pair", "app", "--routes"]);

    let fingerprint = fingerprint_of(&certificate(&session));
    assert_eq!(
        serde_json::from_str::<Value>(&routes).expect("json"),
        json!({"routes": [{"host": "127.0.0.1", "port": port}], "fingerprint": fingerprint})
    );
    assert_eq!(text, format!("127.0.0.1:{port}"));
}

#[test]
fn the_certificate_stays_the_same_after_a_restart() {
    let (session, listener) = serving();
    let before = session.says(&["config", "pair", "app", "--routes", "--json"]);
    let secret = paired(&session, &listener);

    let killed = session.cli(&["kill-server"]);
    assert!(killed.status.success(), "{killed:?}");
    start(&session);
    let after = session.says(&["config", "pair", "app", "--routes", "--json"]);

    assert_eq!(after, before);
    let seen = run(&listener, &secret, &json!(["contract"]));
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
}

#[test]
fn a_new_certificate_forgets_every_app_and_pairs_with_its_own_code() {
    let (session, listener) = serving();
    let old = paired(&session, &listener);
    let args = ["config", "pair", "app", "--new-certificate", "--json"];

    let renewed = pairing_with(&session, &args, |path| {
        let trust = trusting(&certificate(&session));
        connect(&Listener { port: listener.port, trust }, path, None)
    });

    let fresh = Listener { port: listener.port, trust: trusting(&certificate(&session)) };
    assert_eq!(fingerprint_in(&renewed.url), fingerprint_of(&certificate(&session)));
    assert!(String::from_utf8_lossy(&renewed.ended.stderr).contains("the certificate is new"), "{:?}", renewed.ended);
    assert_eq!(connect(&listener, "/cli", None).err(), Some(0));
    assert_eq!(connect(&fresh, "/cli", Some(&old)).err(), Some(403));
    let seen = run(&fresh, &renewed.secret, &json!(["contract"]));
    assert_eq!(seen.texts.last().map(String::as_str), Some(r#"{"exit":0}"#));
}

#[test]
fn kill_server_closes_running_commands_with_going_away() {
    let (session, listener) = serving();
    let secret = paired(&session, &listener);

    let (mut ws, _) = events(&session, &listener, &secret);
    let killed = session.cli(&["kill-server"]);
    let seen = read_all(&mut ws);

    assert!(killed.status.success(), "{killed:?}");
    assert!(seen.texts.iter().all(|text| !text.starts_with(r#"{"exit""#)), "{seen:?}");
    assert_eq!(seen.close, Some(1001));
}

#[test]
fn requests_follow_runs_several_commands_over_one_socket() {
    let (session, listener) = serving();
    let secret = paired(&session, &listener);

    let mut ws = connect(&listener, "/cli", Some(&secret)).expect("an upgrade");
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
