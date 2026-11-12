use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value;

use super::remote::{gone, remote_window, tunnel_args, tunnel_pid};
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

    let closed = listed(&session, &["tunnel", "close", &port.to_string(), "--json"]);

    assert_eq!(closed["tunnels"], serde_json::json!([]));
    wait_until("the tunnel's ssh is gone", || gone(&pid));
}

#[test]
fn opening_a_tunnel_with_no_window_says_why() {
    let app = Harness::start();

    let out = app.session.cli(&["tunnel", "open", "5173"]);

    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("cornercase remote"), "{out:?}");
}
