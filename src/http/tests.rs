use rstest::rstest;

use super::*;
use crate::test_util::FakeHttp;

const SERVICE: Service = Service { name: "Tracker", rejected: "Tracker rejected the token" };

#[rstest]
#[case::with_path("https://api.app.shortcut.com/api/v3", "https://api.app.shortcut.com")]
#[case::without_path("http://127.0.0.1:4000", "http://127.0.0.1:4000")]
fn origin_is_scheme_and_host(#[case] url: &str, #[case] expected: &str) {
    assert_eq!(origin(url), expected);
}

#[test]
fn get_sends_the_headers_and_the_query() {
    let server = FakeHttp::start(vec![("GET /things", 200, r#"{"a":1}"#)]);

    let answer = get(&SERVICE, &format!("{}/things", server.url()), &[("q", "a b")], &[("Token", "t0k")]).expect("get");

    let request = server.request(0);
    assert_eq!((answer.status, answer.json["a"].as_i64()), (200, Some(1)));
    assert!(request.starts_with("GET /things?q=a+b ") || request.starts_with("GET /things?q=a%20b "), "{request}");
    assert!(request.to_lowercase().contains("token: t0k"), "{request}");
}

#[test]
fn get_follows_redirects_unless_asked_not_to() {
    let target = FakeHttp::start(vec![("GET /there", 200, r#"{"a":1}"#)]);
    let server = FakeHttp::start(vec![("GET /here", 302, format!("{}/there", target.url()))]);
    let url = format!("{}/here", server.url());

    let followed = get(&SERVICE, &url, &[], &[]).expect("get");
    let stayed = get_without_redirects(&SERVICE, &url, &[], &[("Token", "t0k")]).err().map(|e| e.to_string());

    assert_eq!(followed.status, 200);
    assert_eq!(
        stayed.as_deref(),
        Some(
            "Tracker answered 302 with a redirect, which is not followed so the token is not sent elsewhere: use the \
             address it leads to"
        )
    );
    assert_eq!(target.requests().len(), 1);
}

#[test]
fn post_sends_json() {
    let server = FakeHttp::start(vec![("POST /graphql", 200, r#"{"data":{}}"#)]);

    post(&SERVICE, &format!("{}/graphql", server.url()), &[], &serde_json::json!({"query": "q"})).expect("post");

    assert!(server.request(0).ends_with(r#"{"query":"q"}"#), "{}", server.request(0));
}

#[rstest]
#[case::unauthorized(401, "Tracker rejected the token")]
#[case::forbidden(403, "Tracker rejected the token")]
#[case::rate_limited(429, "Tracker rate limit reached, try again in a minute")]
fn some_answers_are_errors(#[case] status: u16, #[case] message: &str) {
    let server = FakeHttp::start(vec![("GET /x", status, "{}")]);
    let err = get(&SERVICE, &format!("{}/x", server.url()), &[], &[]).err().map(|e| e.to_string());
    assert_eq!(err.as_deref(), Some(message));
}

#[test]
fn download_reads_the_whole_body() {
    let server = FakeHttp::start(vec![("GET /file", 200, "some bytes")]);

    let bytes = download(&SERVICE, &format!("{}/file", server.url())).expect("download");

    assert_eq!(bytes, b"some bytes");
}

#[test]
fn download_fails_on_a_missing_file() {
    let server = FakeHttp::start(Vec::<(&str, u16, &str)>::new());

    let err = download(&SERVICE, &format!("{}/file", server.url())).err().map(|e| e.to_string());

    assert_eq!(err, Some(format!("Tracker answered 404 for {}/file", server.url())));
}

#[test]
fn an_unreachable_server_says_so() {
    let err = get(&SERVICE, "http://127.0.0.1:1/x", &[], &[]).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.starts_with("could not reach Tracker"), "{err}");
}

#[test]
fn a_key_cannot_leave_its_path_segment() {
    assert_eq!(path_segment("../x?y"), "..%2Fx%3Fy");
}

fn chain(hops: usize) -> (String, Vec<FakeHttp>) {
    let mut servers = vec![FakeHttp::start(vec![("GET /x", 200, "end")])];
    for _ in 0..hops {
        let next = servers.last().expect("a server").url();
        servers.push(FakeHttp::start(vec![("GET /x", 302, format!("{next}/x"))]));
    }
    (format!("{}/x", servers.last().expect("a server").url()), servers)
}

#[test]
fn a_strict_download_follows_three_redirects() {
    let (url, _servers) = chain(3);
    let bytes = download_strictly(&SERVICE, &url, 1024, Duration::from_secs(5)).expect("three hops");
    assert_eq!(bytes, b"end");
}

#[test]
fn a_strict_download_stops_at_the_fourth_redirect() {
    let (url, _servers) = chain(4);
    let err = download_strictly(&SERVICE, &url, 1024, Duration::from_secs(5)).err().map(|e| e.to_string());
    assert!(err.is_some_and(|e| e.starts_with("could not reach Tracker")), "four hops must fail");
}

#[test]
fn a_plain_download_still_follows_longer_chains() {
    let (url, _servers) = chain(4);
    assert_eq!(download_within(&SERVICE, &url, 1024, Duration::from_secs(5)).expect("four hops"), b"end");
}
