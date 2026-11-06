use std::io::{BufRead, BufReader};
use std::net::TcpStream;

use serde_json::{Value, json};

use super::*;

const TOKEN: &str = "MTIzNDU2Nzg5.Gx.SECRET-token";
const APP: &str = r#"{"id":"123456789","owner":{"id":"9"},"integration_types_config":{"0":{},"1":{}},"flags":262144}"#;
const SERVERS: &str = r#"[{"id":"7","name":"home"},{"id":"8","name":"work"}]"#;
const OURS: &str = r#"{"owner_id":"9"}"#;
const MADE: &str = r#"{"id":"800","type":15,"name":"cornercase","available_tags":[
    {"id":"11","name":"working"},{"id":"12","name":"needs you"},{"id":"13","name":"done"}]}"#;

fn route_of(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut route = String::new();
    let _ = reader.read_line(&mut route);
    let mut length = 0;
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
        let lower = header.to_lowercase();
        length = lower.strip_prefix("content-length:").and_then(|n| n.trim().parse().ok()).unwrap_or(length);
        header.clear();
    }
    let _ = reader.read_exact(&mut vec![0; length]);
    route.rsplit_once(' ').map(|(route, _)| route.to_string()).unwrap_or_default()
}

fn answer_of(route: &str) -> (&'static str, &'static str) {
    match route {
        "GET /oauth2/applications/@me" => ("200 OK", APP),
        "GET /users/@me/guilds" => ("200 OK", SERVERS),
        "GET /guilds/7" | "GET /guilds/8" => ("200 OK", OURS),
        "GET /guilds/8/channels" => ("200 OK", "[]"),
        "POST /guilds/8/channels" => ("201 Created", MADE),
        _ => ("404 Not Found", "{}"),
    }
}

fn fake_discord() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let url = format!("http://{}", listener.local_addr().expect("its address"));
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (status, body) = answer_of(&route_of(&stream));
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body.as_bytes()));
        }
    });
    url
}

fn with_a_paired_bot() -> Arc<Session> {
    let session = Session::new();
    let write = |name: &str, content: Value| {
        std::fs::write(session.dir.join(name), content.to_string()).expect("write a file");
    };
    write("secrets.json", json!({ "discord_bot_token": TOKEN }));
    let chat = json!({"id": 5, "user": 9, "name": "ada", "paired": 1});
    write("discord.json", json!({"version": 1, "bot": "cornercase", "chat": chat}));
    write("config.json", json!({"discord_forum": true}));
    let mut start = session.cold_command();
    let started = start.env("CORNERCASE_DISCORD_API", fake_discord()).arg("start-server").output().expect("start");
    assert!(started.status.success(), "start-server failed: {started:?}");
    session
}

fn wait_for_forum(session: &Session, what: &str, cond: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let app: Value = serde_json::from_str(&session.says(&["status", "--app"])).expect("the app's state");
        let forum = app["away"][1]["forum"].clone();
        if cond(&forum) {
            return forum;
        }
        assert!(Instant::now() < deadline, "timed out waiting for: {what}, at {forum}");
        thread::sleep(POLL);
    }
}

#[test]
fn a_script_picks_the_server_of_the_forum_and_follows_it_to_ready() {
    let session = with_a_paired_bot();

    let forum = wait_for_forum(&session, "the servers", |forum| forum.get("servers").is_some());
    assert_eq!(forum["pending"], "pick the server for the forum");
    assert_eq!(forum["servers"], json!([{"id": "7", "name": "home"}, {"id": "8", "name": "work"}]));

    let refused = session.cli(&["config", "forum-server", "9"]);
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(said.contains("`9` is not a server the forum can go in") && said.contains("8 (work)"), "{said}");

    let picked = session.says(&["config", "forum-server", "8", "--json"]);
    assert_eq!(serde_json::from_str::<Value>(&picked).expect("json"), json!({"id": "8", "name": "work"}));

    let forum = wait_for_forum(&session, "the forum", |forum| forum["ready"] == true);
    assert_eq!(forum, json!({"on": true, "ready": true, "guild": "8", "channel": "800", "pending": null}));
    session.wait_for_file("discord.json", "the forum is saved", |text| text.contains(r#""channel":800"#));

    let again = session.cli(&["config", "forum-server", "8"]);
    let said = String::from_utf8_lossy(&again.stderr);
    assert!(!again.status.success() && said.contains("work › #cornercase"), "{again:?}");
}

#[test]
fn without_a_bot_the_pick_is_refused_in_words() {
    let session = Session::new();
    let started = session.cold(&["start-server"]);
    assert!(started.status.success(), "start-server failed: {started:?}");

    let refused = session.cli(&["config", "forum-server", "8"]);

    assert!(!refused.status.success(), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("no Discord bot"), "{refused:?}");
}
