use std::io::BufRead as _;

use serde_json::{Value, json};

use super::browser::{TAILSCALE_READY, fake_tailscale, http_get, open_the_pairing_dialog, window_on};
use super::commands::Followed;
use super::remote::{gone, remotely, ssh_pid};
use super::*;

const PAIR: [&str; 4] = ["config", "pair", "browsers", "--json"];
const TAKEN: &str = "HTTP/1.1 303";
const REFUSED: &str = "HTTP/1.1 403";

fn serving(status: &str) -> (Arc<Session>, u16) {
    let session = Session::new();
    let (port, path) = fake_tailscale(&session, status);
    let mut command = session.cold_command();
    command.env("PATH", path).env("CORNERCASE_LOG", "debug").arg("start-server");
    let started = command.output().expect("run cornercase");
    assert!(started.status.success(), "{started:?}");
    (session, port)
}

fn address(followed: &mut Followed) -> String {
    followed.until("the address", |line| line["url"].is_string());
    followed.seen.iter().find_map(|line| line["url"].as_str()).expect("an address").to_string()
}

fn path_of(url: &str) -> String {
    url.split_once(":8443").expect("the served port").1.to_string()
}

fn pairing(session: &Session) -> (std::process::Child, Followed, String) {
    let mut child = session.spawn(&PAIR);
    let mut followed = Followed::of(&mut child);
    let url = address(&mut followed);
    (child, followed, path_of(&url))
}

fn up_to_the_address(out: &mut impl std::io::BufRead) -> String {
    let mut line = String::new();
    while !line.contains("/pair?t=") {
        line.clear();
        assert_ne!(out.read_line(&mut line).expect("a line"), 0, "the output ended before the address");
    }
    line.trim_end().to_string()
}

fn path_in(line: &str) -> String {
    let shown: Value = serde_json::from_str(line).expect("a JSON line");
    path_of(shown["url"].as_str().expect("an address"))
}

fn spawned(command: &mut std::process::Command) -> std::process::Child {
    let piped = std::process::Stdio::piped;
    let null = std::process::Stdio::null;
    command.stdin(null()).stdout(piped()).stderr(piped()).spawn().expect("run cornercase")
}

fn a_phone_takes_the_code(port: u16, path: &str, child: std::process::Child, followed: &mut Followed) {
    assert!(http_get(port, path).starts_with(TAKEN));
    assert_eq!(followed.next("the end"), json!({"paired": true}));
    let ended = finish_within(child, TIMEOUT);
    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
}

fn the_code_goes(port: u16, path: &str) {
    wait_until("the code stops working", || http_get(port, path).starts_with(REFUSED));
}

#[test]
fn it_says_why_it_cannot() {
    let session = Session::new();
    assert!(session.cold(&["start-server"]).status.success());

    for (key, words) in [
        ("browsers", "remote access is off"),
        ("app", "the phone app's access is off"),
        ("telegram.chat", "paste the bot's token first"),
        ("discord.chat", "paste the bot's token first"),
        ("linear", "cannot be paired"),
    ] {
        let out = session.cli(&["config", "pair", key, "--json"]);

        assert!(refused(&out, 1, words), "{key}: {out:?}");
        assert!(out.stdout.is_empty(), "{key}: {out:?}");
    }
}

#[test]
fn a_browser_taking_the_code_ends_it_and_the_log_never_holds_the_code() {
    let (session, port) = serving(TAILSCALE_READY);
    let (child, mut followed, path) = pairing(&session);
    let shown = followed.seen.last().expect("a line").clone();

    assert_eq!(shown["lines"][1], "the phone needs the Tailscale app, logged in as ada@example.com.");
    a_phone_takes_the_code(port, &path, child, &mut followed);

    assert!(http_get(port, &path).starts_with(REFUSED));
    session.wait_for_file("server.log", "the end is logged", |log| log.contains("pairing ended"));
    let log = std::fs::read_to_string(session.dir.join("server.log")).expect("the log");
    let code = path.rsplit('=').next().expect("a code");
    assert!(log.contains("config-pair"), "{log}");
    assert!(!log.contains(code) && !log.contains("pair?t="), "{log}");
}

#[test]
fn a_person_reads_the_same_without_json() {
    let (session, port) = serving(TAILSCALE_READY);
    let mut child = session.spawn(&["config", "pair", "browsers"]);
    let mut out = std::io::BufReader::new(child.stdout.take().expect("its output"));
    let url = up_to_the_address(&mut out);

    assert!(http_get(port, &path_of(&url)).starts_with(TAKEN));
    let ended = finish_within(child, TIMEOUT);
    let rest: Vec<String> = out.lines().map_while(Result::ok).collect();

    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    assert_eq!(rest.first().map(String::as_str), Some("scan the code with your phone's camera."));
    assert_eq!(rest.last().map(String::as_str), Some("a browser connected"));
}

#[test]
fn the_code_goes_when_its_reader_leaves() {
    let (session, port) = serving(TAILSCALE_READY);
    let mut child = session.spawn(&PAIR);
    let mut out = std::io::BufReader::new(child.stdout.take().expect("its output"));
    let path = path_in(&up_to_the_address(&mut out));

    drop(out);
    let ended = finish_within(child, TIMEOUT);

    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
    the_code_goes(port, &path);
}

#[test]
fn two_commands_share_one_code_until_the_last_goes() {
    let (session, port) = serving(TAILSCALE_READY);
    let (mut first, _first_lines, path) = pairing(&session);
    let (mut second, _second_lines, same) = pairing(&session);
    assert_eq!(same, path);

    first.kill().expect("kill the first");
    let _ = first.wait();
    session.wait_for_file("server.log", "the first is gone", |log| log.contains("its client left"));
    second.kill().expect("kill the second");
    let _ = second.wait();

    the_code_goes(port, &path);
}

#[test]
fn a_server_that_stops_first_is_a_failure_in_words() {
    let (session, _port) = serving(TAILSCALE_READY);
    let (child, _lines, _path) = pairing(&session);

    session.says(&["kill-server", "--yes"]);
    let ended = finish_within(child, TIMEOUT);

    assert!(refused(&ended, 1, "the server stopped before a phone paired"), "{ended:?}");
}

#[test]
fn a_tailscale_that_is_not_ready_ends_it_in_words() {
    let (session, _port) = serving(&TAILSCALE_READY.replace("Running", "Stopped"));
    let out = session.cli(&PAIR);

    let lines: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    let why = "tailscale is not connected (Stopped): open it and log in";
    assert!(refused(&out, 1, why), "{out:?}");
    assert_eq!(lines.first(), Some(&json!({"url": null, "lines": ["starting Tailscale…"]})), "{out:?}");
    assert_eq!(lines.last(), Some(&json!({"paired": false, "error": why})), "{out:?}");
}

#[test]
fn through_ssh_the_address_is_the_servers_and_it_ends_there_too() {
    let (session, port) = serving(TAILSCALE_READY);
    let mut child = spawned(&mut remotely(&session, &PAIR));
    let mut followed = Followed::of(&mut child);
    let path = path_of(&address(&mut followed));

    a_phone_takes_the_code(port, &path, child, &mut followed);
}

#[test]
fn over_ssh_its_reader_leaving_drops_the_code_there() {
    let (session, port) = serving(TAILSCALE_READY);
    let mut child = spawned(&mut remotely(&session, &PAIR));
    let mut out = std::io::BufReader::new(child.stdout.take().expect("its output"));
    let path = path_in(&up_to_the_address(&mut out));
    let proxy = ssh_pid(&session);

    drop(out);
    let ended = finish_within(child, TIMEOUT);

    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    wait_until("the proxy there ends", || gone(&proxy));
    the_code_goes(port, &path);
}

#[test]
fn the_window_stays_as_it_is_until_it_opens_its_dialog_and_closing_that_keeps_the_code() {
    let session = Session::new();
    let (port, path) = fake_tailscale(&session, TAILSCALE_READY);
    let mut ui = window_on(&session, &path);
    let (child, mut followed, code_path) = pairing(&session);
    assert!(!ui.text().contains("pair?t="), "{}", ui.text());

    let token = open_the_pairing_dialog(&mut ui);
    assert_eq!(token, code_path.rsplit('=').next().expect("a code"));
    ui.click(ui.position_of("done"));
    ui.wait_for("the dialog closes", |s| !s.contains("pair?t="));

    a_phone_takes_the_code(port, &code_path, child, &mut followed);
    ui.wait_for("the window says a browser connected", |s| s.contains("a browser connected"));
}
