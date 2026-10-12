use super::*;

fn request(lines: &[&str]) -> Request {
    Request::parse(format!("{}\r\n\r\n", lines.join("\r\n")).as_bytes()).expect("a request")
}

#[test]
fn reads_the_path_query_and_headers() {
    let r = request(&["GET /pair?t=abc&x HTTP/1.1", "Host: mac:8443", "Tailscale-User-Login: ada@example.com"]);

    assert_eq!(
        (r.method.as_str(), r.path.as_str(), r.query("t"), r.query("x")),
        ("GET", "/pair", Some("abc"), Some(""))
    );
    assert_eq!(r.header("tailscale-user-login"), Some("ada@example.com"));
}

#[test]
fn finds_a_cookie_among_others() {
    let r = request(&["GET / HTTP/1.1", "Cookie: theme=dark; cornercase=id.secret", "Cookie: other=1"]);

    assert_eq!((r.cookie("cornercase"), r.cookie("other"), r.cookie("none")), (Some("id.secret"), Some("1"), None));
}

#[test]
fn an_origin_must_match_the_host_it_was_sent_to() {
    let tailnet = request(&["GET /ws HTTP/1.1", "Host: mac.ts.net:8443", "Origin: https://mac.ts.net:8443"]);
    let forwarded = request(&[
        "GET /ws HTTP/1.1",
        "Host: 127.0.0.1:47681",
        "X-Forwarded-Host: mac.ts.net:8443",
        "Origin: https://mac.ts.net:8443",
    ]);
    let other = request(&["GET /ws HTTP/1.1", "Host: mac.ts.net:8443", "Origin: https://evil.example"]);
    let scheme = request(&["GET /ws HTTP/1.1", "Host: mac.ts.net:8443", "Origin: file://mac.ts.net:8443"]);
    let none = request(&["GET /ws HTTP/1.1", "Host: mac.ts.net:8443"]);

    assert_eq!(
        [tailnet.same_origin(), forwarded.same_origin(), other.same_origin(), scheme.same_origin(), none.same_origin()],
        [true, true, false, false, false]
    );
}

#[test]
fn a_proxied_request_says_so() {
    let local = request(&["GET / HTTP/1.1", "Host: 127.0.0.1:47681"]);
    let proxied = request(&["GET / HTTP/1.1", "Host: mac.ts.net:8443", "X-Forwarded-For: 100.64.0.2"]);

    assert_eq!((local.forwarded(), proxied.forwarded()), (false, true));
}

#[test]
fn a_head_that_never_ends_is_dropped() {
    let endless = vec![b'a'; MAX_HEAD + 10];

    assert_eq!(Request::read(&mut endless.as_slice()).expect("read"), None);
}

#[test]
fn a_response_says_its_length_and_status() {
    let mut out = Vec::new();
    Response::redirect("/").with("Set-Cookie", "a=b".into()).send(&mut out).expect("sent");

    let text = String::from_utf8(out).expect("utf-8");
    assert!(text.starts_with("HTTP/1.1 303 See Other\r\nContent-Length: 0\r\n"));
    assert!(text.contains("\r\nLocation: /\r\n") && text.contains("\r\nSet-Cookie: a=b\r\n"));
}
