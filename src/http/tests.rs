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
    let stayed = get_without_redirects(&SERVICE, &url, &[], &[]).expect("get without redirects");

    assert_eq!((followed.status, stayed.status, target.requests().len()), (200, 302, 1));
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
