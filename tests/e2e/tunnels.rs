use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use super::remote::{
    gone, kept_tunnels, remote_window, remotely, reverse_args, reverse_pid, tunnel_args, tunnel_pid, tunnel_pids,
};
use super::*;

const SERVE: &str = "python3 -c 'import socket; s = socket.socket(); s.bind((\"127.0.0.1\", 0)); s.listen(); [(c.sendall(b\"hello\"), c.close()) for c in iter(lambda: s.accept()[0], None)]'";

fn listed(session: &Session, command: &[&str]) -> Value {
    serde_json::from_str(&session.says(command)).expect("json")
}

fn served_port(session: &Session, pane: u64) -> u16 {
    let mut port = None;
    wait_until("the server listens", || {
        let ports = listed(session, &["ports", "--json"]);
        port =
            ports["ports"].as_array().and_then(|all| all.iter().find(|p| p["pane"] == pane)).map(|p| p["port"].clone());
        port.is_some()
    });
    u16::try_from(port.and_then(|p| p.as_u64()).expect("a port")).expect("a port")
}

#[test]
fn a_program_opens_lists_and_closes_a_tunnel_through_the_window() {
    let session = Session::new();
    let _window = remote_window(&session);
    let pane: u64 = session.says(&["new-tab", "--name", "server", "--", SERVE]).parse().expect("a pane id");
    let port = served_port(&session, pane);

    let opened = listed(&session, &["tunnel", "open", &port.to_string(), "--json"]);

    let tunnel = &opened["tunnels"][0];
    assert_eq!((tunnel["port"].as_u64(), tunnel["state"].as_str()), (Some(u64::from(port)), Some("up")));
    let local = u16::try_from(tunnel["local"].as_u64().expect("a local port")).expect("a port");
    assert_ne!(local, port, "the port is taken on this computer");
    let args = tunnel_args(&session, port);
    for word in ["BatchMode=yes", "ExitOnForwardFailure=yes", "ControlPath=none", "-N", "--", "devbox"] {
        assert!(args.iter().any(|arg| arg == word), "{word} in {args:?}");
    }
    assert!(args.contains(&format!("127.0.0.1:{local}:localhost:{port}")), "{args:?}");
    let mut stream = TcpStream::connect(("127.0.0.1", local)).expect("reach the tunnel");
    stream.set_read_timeout(Some(Duration::from_secs(20))).expect("a timeout");
    let mut said = [0u8; 5];
    stream.read_exact(&mut said).expect("read through the tunnel");
    assert_eq!(&said, b"hello");
    assert_eq!(listed(&session, &["tunnels", "--json"])["tunnels"][0]["state"], "up");
    let pid = tunnel_pid(&session, port);

    closed_then_forgotten(&session, port, &pid);
}

fn closed_then_forgotten(session: &Session, port: u16, pid: &str) {
    let closed = listed(session, &["tunnel", "close", &port.to_string(), "--json"]);

    assert_eq!(closed["tunnels"][0]["state"], "closed");
    wait_until("the tunnel's ssh is gone", || gone(pid));
    assert_eq!(listed(session, &["tunnels", "--json"])["tunnels"][0]["state"], "closed");
    let kept = &kept_tunnels(session)["devbox"][0];
    assert_eq!(
        (kept["port"].as_u64(), kept["closed"].as_bool(), kept.get("pid")),
        (Some(u64::from(port)), Some(true), None)
    );

    let forgotten = listed(session, &["tunnel", "forget", &port.to_string(), "--json"]);

    assert_eq!(forgotten["tunnels"], serde_json::json!([]));
    assert!(kept_tunnels(session).get("devbox").is_none(), "{}", kept_tunnels(session));
}

fn local_of(session: &Session) -> Option<u16> {
    let list = listed(session, &["tunnels", "--json"]);
    list["tunnels"][0]["local"].as_u64().and_then(|local| u16::try_from(local).ok())
}

fn read_through(local: u16) -> [u8; 5] {
    wait_until("the local port accepts", || TcpStream::connect(("127.0.0.1", local)).is_ok());
    let mut stream = TcpStream::connect(("127.0.0.1", local)).expect("reach the tunnel");
    stream.set_read_timeout(Some(Duration::from_secs(20))).expect("a timeout");
    let mut said = [0u8; 5];
    stream.read_exact(&mut said).expect("read through the tunnel");
    said
}

#[test]
fn a_second_window_shares_the_tunnel_and_takes_it_over_when_the_first_quits() {
    let session = Session::new();
    let mut first = remote_window(&session);
    let pane: u64 = session.says(&["new-tab", "--name", "server", "--", SERVE]).parse().expect("a pane id");
    let port = served_port(&session, pane);
    let opened = listed(&session, &["tunnel", "open", &port.to_string(), "--json"]);
    let local = u16::try_from(opened["tunnels"][0]["local"].as_u64().expect("a local port")).expect("a port");
    let first_pid = tunnel_pid(&session, port);

    let _second = remote_window(&session);

    wait_until("both windows list it up on the same port", || {
        let list = listed(&session, &["tunnels", "--json"]);
        let all = list["tunnels"].as_array().expect("a list");
        all.len() == 2 && all.iter().all(|t| t["state"] == "up" && t["local"].as_u64() == Some(u64::from(local)))
    });
    assert_eq!(tunnel_pids(&session, port).len(), 1, "the second window starts no ssh");

    first.quit();
    first.wait_exit("the first window quits");

    wait_until("the second window took the tunnel over", || {
        let list = listed(&session, &["tunnels", "--json"]);
        let all = list["tunnels"].as_array().expect("a list");
        let new_ssh = tunnel_pids(&session, port).len() == 2;
        new_ssh && all.len() == 1 && all[0]["state"] == "up" && all[0]["local"].as_u64() == Some(u64::from(local))
    });
    assert!(gone(&first_pid));
    let kept = &kept_tunnels(&session)["devbox"][0];
    assert_eq!(kept["pid"].to_string(), tunnel_pid(&session, port));
    assert_eq!(&read_through(local), b"hello");
}

#[test]
fn two_attached_windows_asked_to_open_a_tunnel_end_with_one_ssh_between_them() {
    let session = Session::new();
    let _first = remote_window(&session);
    let _second = remote_window(&session);
    let pane: u64 = session.says(&["new-tab", "--name", "server", "--", SERVE]).parse().expect("a pane id");
    let port = served_port(&session, pane);

    listed(&session, &["tunnel", "open", &port.to_string(), "--json"]);

    wait_until("both windows list it up on one local port", || {
        let list = listed(&session, &["tunnels", "--json"]);
        let all = list["tunnels"].as_array().expect("a list");
        let locals: Vec<Option<u64>> = all.iter().map(|t| t["local"].as_u64()).collect();
        all.len() == 2 && all.iter().all(|t| t["state"] == "up") && locals[0] == locals[1]
    });
    let local = local_of(&session).expect("a local port");
    wait_until("one ssh is left", || tunnel_pids(&session, port).iter().filter(|pid| !gone(pid)).count() == 1);
    let pids = tunnel_pids(&session, port);
    let alive: Vec<&String> = pids.iter().filter(|pid| !gone(pid)).collect();
    assert_eq!(alive.len(), 1, "one ssh is left of {pids:?}");
    assert_eq!(kept_tunnels(&session)["devbox"][0]["pid"].to_string(), *alive[0]);
    assert_eq!(&read_through(local), b"hello");
}

#[test]
fn a_new_window_brings_back_the_tunnels_it_remembered() {
    let session = Session::new();
    let mut first = remote_window(&session);
    let pane: u64 = session.says(&["new-tab", "--name", "server", "--", SERVE]).parse().expect("a pane id");
    let port = served_port(&session, pane);
    listed(&session, &["tunnel", "open", &port.to_string(), "--json"]);
    let local = local_of(&session).expect("a local port");
    let pid = tunnel_pid(&session, port);
    first.quit();
    first.wait_exit("the window quits");
    wait_until("the tunnel's ssh is gone", || gone(&pid));

    let _again = remote_window(&session);

    wait_until("the tunnel is back on the same port", || {
        let list = listed(&session, &["tunnels", "--json"]);
        list["tunnels"][0]["state"] == "up" && local_of(&session) == Some(local)
    });
    assert_eq!(&read_through(local), b"hello");
}

type Streaming = (std::process::Child, Value, BufReader<std::process::ChildStdout>);

fn streaming(session: &Session, args: &[&str]) -> Streaming {
    let mut child = remotely(session, args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run the command");
    let mut reader = BufReader::new(child.stdout.take().expect("piped"));
    let first = next_line(&mut reader);
    (child, first, reader)
}

fn next_line(reader: &mut BufReader<std::process::ChildStdout>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("one line");
    serde_json::from_str(&line).expect("a json line")
}

fn held(session: &Session, port: u16) -> Streaming {
    streaming(session, &["tunnel", "hold", &port.to_string()])
}

fn exit_of(child: &mut std::process::Child) -> Option<i32> {
    let mut code = None;
    wait_until("the command ends", || match child.try_wait().expect("wait") {
        Some(status) => {
            code = status.code();
            true
        }
        None => false,
    });
    code
}

fn serving(app: &Harness) -> u16 {
    let pane: u64 = app.session.says(&["new-tab", "--name", "server", "--", SERVE]).parse().expect("a pane id");
    served_port(&app.session, pane)
}

#[test]
fn a_program_holds_a_tunnel_without_a_window_until_it_is_closed() {
    let app = Harness::start();
    let port = serving(&app);

    let (mut child, line, _reader) = held(&app.session, port);

    let local = line["local"].as_u64().expect("a local port");
    assert_eq!(line["port"].as_u64(), Some(u64::from(port)));
    assert_eq!(line["url"].as_str(), Some(format!("http://localhost:{local}").as_str()));
    wait_until("the server lists the hold", || {
        listed(&app.session, &["tunnels", "--json"])["tunnels"][0]["kind"].as_str() == Some("hold")
    });
    assert_eq!(listed(&app.session, &["tunnels", "--json"])["windows"].as_u64(), Some(0));
    let pid = tunnel_pid(&app.session, port);

    app.session.says(&["tunnel", "close", &port.to_string()]);

    assert_eq!(exit_of(&mut child), Some(0));
    wait_until("the tunnel's ssh is gone", || gone(&pid));
    assert_eq!(listed(&app.session, &["tunnels", "--json"])["tunnels"], serde_json::json!([]));
}

#[test]
fn a_held_tunnel_ends_with_zero_when_killed() {
    let app = Harness::start();
    let port = serving(&app);
    let (mut child, _line, _reader) = held(&app.session, port);
    let pid = tunnel_pid(&app.session, port);

    let killed = std::process::Command::new("kill").args(["-TERM", &child.id().to_string()]).status().expect("kill");

    assert!(killed.success());
    assert_eq!(exit_of(&mut child), Some(0));
    wait_until("the tunnel's ssh is gone", || gone(&pid));
    wait_until("the list is empty", || {
        listed(&app.session, &["tunnels", "--json"])["tunnels"] == serde_json::json!([])
    });
}

#[test]
fn a_held_tunnel_whose_ssh_fails_ends_with_one() {
    let app = Harness::start();
    let port = serving(&app);
    std::fs::write(app.session.dir.join("tunnel.fails"), "").expect("make the tunnel's ssh fail");

    let out = remotely(&app.session, &["tunnel", "hold", &port.to_string()]).output().expect("run tunnel hold");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr.contains("could not tunnel port") && stderr.contains("Connection refused"), "{stderr}");
}

#[test]
fn a_held_tunnel_ends_when_its_reader_leaves() {
    let app = Harness::start();
    let port = serving(&app);
    let (mut child, _line, reader) = held(&app.session, port);
    let pid = tunnel_pid(&app.session, port);

    drop(reader);

    assert_eq!(exit_of(&mut child), Some(0));
    wait_until("the tunnel's ssh is gone", || gone(&pid));
}

#[test]
fn tunnel_hold_needs_remote() {
    let session = Session::new();

    for args in [&["tunnel", "hold", "5173"][..], &["tunnel", "hold", "--reverse", "9222"]] {
        let out = session.cli(args);

        assert_eq!(out.status.code(), Some(2), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("needs --remote"), "{out:?}");
    }
}

fn free_port() -> u16 {
    let socket = TcpListener::bind(("127.0.0.1", 0)).expect("bind a port");
    socket.local_addr().expect("the port").port()
}

#[test]
fn a_held_tunnel_takes_the_local_port_it_asks_for_when_its_port_is_taken_here() {
    let app = Harness::start();
    let port = serving(&app);
    let wanted = free_port();

    let (_child, line, _reader) =
        streaming(&app.session, &["tunnel", "hold", &port.to_string(), "--local", &wanted.to_string()]);

    assert_eq!(line["local"].as_u64(), Some(u64::from(wanted)), "{line}");
    assert_eq!(&read_through(wanted), b"hello");
}

#[test]
fn opening_a_tunnel_with_no_window_says_why() {
    let app = Harness::start();

    let out = app.session.cli(&["tunnel", "open", "5173"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cornercase remote") && stderr.contains("tunnel orders"), "{out:?}");
}

fn taking_orders(session: &Session) -> Streaming {
    streaming(session, &["tunnel", "orders"])
}

fn asking(session: &Session, port: u16) -> std::process::Child {
    session
        .command()
        .args(["tunnel", "open", &port.to_string(), "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run tunnel open")
}

fn finished(mut child: std::process::Child) -> (Option<i32>, String, String) {
    let code = exit_of(&mut child);
    let (mut out, mut err) = (String::new(), String::new());
    child.stdout.take().expect("piped").read_to_string(&mut out).expect("stdout");
    child.stderr.take().expect("piped").read_to_string(&mut err).expect("stderr");
    (code, out, err)
}

#[test]
fn a_program_with_no_window_takes_an_open_and_answers_it_with_a_hold() {
    let app = Harness::start();
    let port = serving(&app);
    let (mut orders, first, mut reader) = taking_orders(&app.session);
    assert_eq!((first["takers"].as_u64(), first["windows"].as_u64()), (Some(1), Some(0)), "{first}");

    let asked = asking(&app.session, port);

    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "open", "port": port}));
    let (mut hold, line, _held) = held(&app.session, port);
    let (code, out, _) = finished(asked);
    assert_eq!(code, Some(0));
    let answer: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(answer["tunnels"][0]["kind"], "hold");
    assert_eq!(answer["tunnels"][0]["local"], line["local"]);
    let local = u16::try_from(line["local"].as_u64().expect("a local port")).expect("a port");
    assert_eq!(&read_through(local), b"hello");
    app.session.says(&["tunnel", "close", &port.to_string()]);
    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "close", "port": port}));
    assert_eq!(exit_of(&mut hold), Some(0));
    drop(reader);
    assert_eq!(exit_of(&mut orders), Some(0));
    wait_until("no program takes orders", || listed(&app.session, &["tunnels", "--json"])["takers"] == 0);
}

#[test]
fn a_program_that_cannot_hold_the_port_fails_the_open_with_its_reason() {
    let app = Harness::start();
    let (_orders, _first, mut reader) = taking_orders(&app.session);
    let asked = asking(&app.session, 3999);
    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "open", "port": 3999}));

    let reported = remotely(&app.session, &["tunnel", "failed", "3999", "the user said no"]).output().expect("run");

    assert!(reported.status.success(), "{reported:?}");
    let (code, _, err) = finished(asked);
    assert_eq!(code, Some(1));
    assert!(err.contains("could not tunnel port 3999: the user said no"), "{err}");
}

#[test]
fn a_hold_whose_ssh_fails_fails_the_open_it_answers() {
    let app = Harness::start();
    let port = serving(&app);
    let (_orders, _first, mut reader) = taking_orders(&app.session);
    let asked = asking(&app.session, port);
    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "open", "port": port}));
    std::fs::write(app.session.dir.join("tunnel.fails"), "").expect("make the tunnel's ssh fail");

    let held = remotely(&app.session, &["tunnel", "hold", &port.to_string()]).output().expect("run tunnel hold");

    assert_eq!(held.status.code(), Some(1), "{held:?}");
    let (code, _, err) = finished(asked);
    assert_eq!(code, Some(1));
    assert!(err.contains("Connection refused"), "{err}");
}

#[test]
fn a_program_that_stops_taking_orders_answers_what_waited_and_no_longer_counts() {
    let app = Harness::start();
    let (orders, _first, mut reader) = taking_orders(&app.session);
    let asked = asking(&app.session, 3999);
    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "open", "port": 3999}));

    let killed = std::process::Command::new("kill").args(["-TERM", &orders.id().to_string()]).status().expect("kill");

    assert!(killed.success());
    let (code, _, err) = finished(asked);
    assert_eq!(code, Some(1));
    assert!(err.contains("no program answered"), "{err}");
    wait_until("no program takes orders", || listed(&app.session, &["tunnels", "--json"])["takers"] == 0);
    let again = app.session.cli(&["tunnel", "open", "3999"]);
    assert_eq!(again.status.code(), Some(1), "{again:?}");
    assert!(String::from_utf8_lossy(&again.stderr).contains("tunnel orders"), "{again:?}");
}

#[test]
fn tunnel_orders_needs_remote() {
    let session = Session::new();

    let out = session.cli(&["tunnel", "orders"]);

    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs --remote"), "{out:?}");
}

fn serving_here() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind a port");
    let port = listener.local_addr().expect("the port").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.write_all(b"hello");
        }
    });
    port
}

fn held_reverse(session: &Session, local: u16, there: Option<u16>) -> Streaming {
    let local = local.to_string();
    let there = there.map(|there| there.to_string());
    let mut args = vec!["tunnel", "hold", "--reverse", &local];
    if let Some(there) = &there {
        args.extend(["--there", there]);
    }
    streaming(session, &args)
}

fn port_of(line: &Value) -> u16 {
    u16::try_from(line["port"].as_u64().expect("a port")).expect("a port")
}

fn assert_reverse_line(session: &Session, local: u16) {
    let args = reverse_args(session, local);
    let wanted = ["-R", "127.0.0.1:0:localhost", "LogLevel=INFO", "ExitOnForwardFailure=yes", "ControlPath=none"];
    for word in wanted.into_iter().chain(["BatchMode=yes", "-N", "--", "devbox"]) {
        assert!(args.iter().any(|arg| arg.contains(word)), "{word} in {args:?}");
    }
}

fn assert_reverse_listed(session: &Session) {
    let list = listed(session, &["tunnels", "--json"]);
    let tunnel = &list["tunnels"][0];
    assert_eq!((tunnel["kind"].as_str(), tunnel["direction"].as_str()), (Some("hold"), Some("reverse")));
}

#[test]
fn a_program_holds_a_reverse_tunnel_until_it_is_closed() {
    let app = Harness::start();
    let local = serving_here();

    let (mut child, line, _reader) = held_reverse(&app.session, local, None);

    let port = port_of(&line);
    assert_ne!(port, 0);
    assert_eq!(line["local"].as_u64(), Some(u64::from(local)));
    assert_eq!(line["direction"].as_str(), Some("reverse"));
    assert_eq!(line["url"].as_str(), Some(format!("http://127.0.0.1:{port}").as_str()));
    assert_eq!(&read_through(port), b"hello");
    assert_reverse_listed(&app.session);
    assert_reverse_line(&app.session, local);
    let pid = reverse_pid(&app.session, local);
    let plain = app.session.cli(&["tunnel", "close", &port.to_string()]);
    assert_eq!(plain.status.code(), Some(1), "{plain:?}");
    assert!(child.try_wait().expect("wait").is_none(), "a plain close does not reach a reverse hold");

    app.session.says(&["tunnel", "close", "--reverse", &port.to_string()]);

    assert_eq!(exit_of(&mut child), Some(0));
    wait_until("the tunnel's ssh is gone", || gone(&pid));
    assert_eq!(listed(&app.session, &["tunnels", "--json"])["tunnels"], serde_json::json!([]));
}

#[test]
fn a_reverse_tunnel_on_a_chosen_port() {
    let app = Harness::start();
    let local = serving_here();
    let there = free_port();

    let (_child, line, _reader) = held_reverse(&app.session, local, Some(there));

    assert_eq!(port_of(&line), there);
    let args = reverse_args(&app.session, local);
    assert!(args.contains(&"LogLevel=DEBUG1".to_string()), "{args:?}");
    assert!(args.contains(&format!("127.0.0.1:{there}:localhost:{local}")), "{args:?}");
    assert_eq!(&read_through(there), b"hello");
}

#[test]
fn a_reverse_tunnel_on_a_port_taken_there_fails_at_once() {
    let app = Harness::start();
    let local = serving_here();
    let taken = TcpListener::bind(("127.0.0.1", 0)).expect("take a port");
    let there = taken.local_addr().expect("the port").port();

    let (local, there) = (local.to_string(), there.to_string());
    let out = remotely(&app.session, &["tunnel", "hold", "--reverse", &local, "--there", &there])
        .output()
        .expect("run tunnel hold");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr.contains(&format!("remote port forwarding failed for listen port {there}")), "{stderr}");
    assert_eq!(listed(&app.session, &["tunnels", "--json"])["tunnels"], serde_json::json!([]));
}

#[test]
fn a_forward_and_a_reverse_tunnel_of_one_port_are_kept_apart() {
    let app = Harness::start();
    let local = serving_here();
    let (mut reverse, line, _reverse_reader) = held_reverse(&app.session, local, None);
    let port = port_of(&line);

    let (mut forward, held_line, _forward_reader) = held(&app.session, port);

    let through = u16::try_from(held_line["local"].as_u64().expect("a local port")).expect("a port");
    assert_ne!(through, port, "the reverse relay listens on {port} here");
    assert_eq!(&read_through(through), b"hello");
    let tunnels = listed(&app.session, &["tunnels", "--json"]);
    let all = tunnels["tunnels"].as_array().expect("a list");
    let reversed = all.iter().filter(|tunnel| tunnel["direction"] == "reverse").count();
    assert_eq!((all.len(), reversed), (2, 1), "{tunnels}");

    app.session.says(&["tunnel", "close", &port.to_string()]);

    assert_eq!(exit_of(&mut forward), Some(0));
    assert!(reverse.try_wait().expect("wait").is_none());
    assert_eq!(listed(&app.session, &["tunnels", "--json"])["tunnels"][0]["direction"], "reverse");
    app.session.says(&["tunnel", "close", "--reverse", &port.to_string()]);
    assert_eq!(exit_of(&mut reverse), Some(0));
}

#[test]
fn a_reverse_hold_never_answers_an_open() {
    let app = Harness::start();
    let port = serving(&app);
    let (_orders, _first, mut reader) = taking_orders(&app.session);
    let asked = asking(&app.session, port);
    assert_eq!(next_line(&mut reader), serde_json::json!({"order": "open", "port": port}));
    std::fs::write(app.session.dir.join("tunnel.fails"), "").expect("make the tunnel's ssh fail");
    let (local, there) = (serving_here().to_string(), port.to_string());

    let failed = remotely(&app.session, &["tunnel", "hold", "--reverse", &local, "--there", &there])
        .output()
        .expect("run tunnel hold");

    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    std::fs::remove_file(app.session.dir.join("tunnel.fails")).expect("let the tunnel's ssh work");
    let (_hold, _line, _held) = held(&app.session, port);
    let (code, out, err) = finished(asked);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(serde_json::from_str::<Value>(&out).expect("json")["tunnels"][0]["kind"], "hold");
}
