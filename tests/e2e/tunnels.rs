use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value;

use super::remote::{gone, kept_tunnels, remote_window, remotely, tunnel_args, tunnel_pid, tunnel_pids};
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

fn held(session: &Session, port: u16) -> (std::process::Child, Value, BufReader<std::process::ChildStdout>) {
    let mut child = remotely(session, &["tunnel", "hold", &port.to_string()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run tunnel hold");
    let mut line = String::new();
    let mut reader = BufReader::new(child.stdout.take().expect("piped"));
    reader.read_line(&mut line).expect("one line");
    (child, serde_json::from_str(&line).expect("a json line"), reader)
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

    let out = session.cli(&["tunnel", "hold", "5173"]);

    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs --remote"), "{out:?}");
}

#[test]
fn opening_a_tunnel_with_no_window_says_why() {
    let app = Harness::start();

    let out = app.session.cli(&["tunnel", "open", "5173"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("cornercase remote"), "{out:?}");
}
