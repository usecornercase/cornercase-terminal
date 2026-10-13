use super::*;

fn http_get(port: u16, path: &str) -> String {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect to the web port");
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").expect("send the request");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("read the answer");
    answer
}

fn pairing_token(screen: &str) -> String {
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines.iter().position(|l| l.contains("pair?t=")).expect("the pairing address");
    let hex = |s: &str| {
        s.chars().skip_while(|c| !c.is_ascii_hexdigit()).take_while(char::is_ascii_hexdigit).collect::<String>()
    };
    let first = hex(lines[at].split("pair?t=").nth(1).unwrap_or_default());
    let rest = lines.get(at + 1).and_then(|l| l.split('│').nth(1)).map(hex).unwrap_or_default();
    format!("{first}{rest}").chars().take(32).collect()
}

#[test]
fn a_paired_browser_types_into_the_shell_over_a_websocket() {
    let session = Session::new();
    let port = TcpListener::bind(("127.0.0.1", 0)).expect("bind").local_addr().expect("address").port();
    let bin = session.dir.join("bin");
    std::fs::create_dir_all(&bin).expect("bin folder");
    let status = r#"{"BackendState":"Running","CertDomains":["mac.tail1234.ts.net"],"Self":{"DNSName":"mac.tail1234.ts.net.","UserID":7},"User":{"7":{"LoginName":"ada@example.com"}}}"#;
    let script = bin.join("tailscale");
    write_executable(&script, &format!("#!/bin/sh\n[ \"$1\" = status ] && echo '{status}'\nexit 0\n"));
    let config = format!(r#"{{"remote": true, "remote_port": {port}}}"#);
    std::fs::write(session.dir.join("config.json"), config).expect("config");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let mut ui = Harness::open_with(Arc::clone(&session), ROWS, COLS, &[("PATH", &path)]);
    ui.wait_for("the remote button shows", |s| s.contains("Remote"));

    ui.click(ui.position_of("Remote"));
    ui.wait_for("the remote menu opens", |s| s.contains("open in your phone's browser"));
    ui.click(ui.position_of("open in your phone's browser"));
    ui.wait_for("the pairing address shows", |s| s.contains("pair?t="));
    let token = pairing_token(&ui.text());
    let paired = http_get(port, &format!("/pair?t={token}"));
    let cookie = paired
        .lines()
        .find_map(|l| l.strip_prefix("Set-Cookie: "))
        .and_then(|c| c.split(';').next())
        .expect("a cookie for the paired browser")
        .to_string();

    let mut request = tungstenite::client::IntoClientRequest::into_client_request(format!("ws://127.0.0.1:{port}/ws"))
        .expect("a websocket request");
    request.headers_mut().insert("Cookie", cookie.parse().expect("cookie header"));
    request.headers_mut().insert("Origin", format!("http://127.0.0.1:{port}").parse().expect("origin header"));
    let stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream.set_read_timeout(Some(TIMEOUT)).expect("read timeout");
    let (mut ws, _) = tungstenite::client::client(request, stream).expect("websocket handshake");
    ws.send(tungstenite::Message::text(r#"{"type":"hello","cols":100,"rows":24}"#)).expect("hello");
    ws.send(tungstenite::Message::binary(b"echo web-\"\"works\r".to_vec())).expect("keys");
    let mut seen = Vec::new();
    let deadline = Instant::now() + TIMEOUT;
    while !String::from_utf8_lossy(&seen).contains("web-works") {
        assert!(Instant::now() < deadline, "the shell's answer never reached the browser");
        if let tungstenite::Message::Binary(bytes) = ws.read().expect("a message") {
            seen.extend_from_slice(&bytes);
        }
    }
    ui.wait_for("the window shows the browser is attached", |s| s.contains("Remote ●"));
}
