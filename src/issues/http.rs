use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::http::Response;

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;

pub struct Service {
    pub name: &'static str,
    pub rejected: &'static str,
}

pub struct Answer {
    pub status: u16,
    pub json: Value,
}

impl Answer {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

fn agent(timeout: Duration) -> Agent {
    Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).build().into()
}

pub fn get(service: &Service, url: &str, query: &[(&str, &str)], headers: &[(&str, &str)]) -> Result<Answer> {
    let mut request = agent(TIMEOUT).get(url).header("Accept", "application/json");
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    finish(service, request.query_pairs(query.iter().copied()).call())
}

pub fn post(service: &Service, url: &str, headers: &[(&str, &str)], body: &Value) -> Result<Answer> {
    let mut request = agent(TIMEOUT).post(url).header("Accept", "application/json");
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    finish(service, request.content_type("application/json").send(body.to_string()))
}

pub fn download(service: &Service, url: &str) -> Result<Vec<u8>> {
    let mut response = reached(service, agent(DOWNLOAD_TIMEOUT).get(url).call(), DOWNLOAD_TIMEOUT)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::Api(format!("{} answered {status} for {url}", service.name)));
    }
    response
        .body_mut()
        .with_config()
        .limit(DOWNLOAD_LIMIT)
        .read_to_vec()
        .map_err(|e| Error::Api(format!("could not download from {}: {e}", service.name)))
}

fn reached(
    service: &Service,
    result: std::result::Result<Response<ureq::Body>, ureq::Error>,
    timeout: Duration,
) -> Result<Response<ureq::Body>> {
    result.map_err(|e| match e {
        ureq::Error::Timeout(_) => {
            Error::Api(format!("{} did not answer within {} s", service.name, timeout.as_secs()))
        }
        e => Error::Api(format!("could not reach {}: {e}", service.name)),
    })
}

fn finish(service: &Service, result: std::result::Result<Response<ureq::Body>, ureq::Error>) -> Result<Answer> {
    let mut response = reached(service, result, TIMEOUT)?;
    let status = response.status().as_u16();
    if status == 401 || status == 403 {
        return Err(Error::Api(service.rejected.into()));
    }
    if status == 429 {
        return Err(Error::Api(format!("{} rate limit reached, try again in a minute", service.name)));
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Api(format!("could not read the answer of {}: {e}", service.name)))?;
    let json = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(Answer { status, json })
}

pub fn into_json(answer: Answer, missing: &str, failure: impl Fn(&Answer) -> Error) -> Result<Value> {
    if answer.status == 404 {
        return Err(Error::Api(missing.into()));
    }
    if !answer.ok() {
        return Err(failure(&answer));
    }
    Ok(answer.json)
}

pub fn origin(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    url[after_scheme..].find('/').map_or(url, |i| &url[..after_scheme + i])
}

pub fn text(value: &Value, pointer: &str) -> String {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_string()
}

pub fn path_segment(key: &str) -> String {
    key.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
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

        let answer =
            get(&SERVICE, &format!("{}/things", server.url()), &[("q", "a b")], &[("Token", "t0k")]).expect("get");

        let request = server.request(0);
        assert_eq!((answer.status, answer.json["a"].as_i64()), (200, Some(1)));
        assert!(request.starts_with("GET /things?q=a+b ") || request.starts_with("GET /things?q=a%20b "), "{request}");
        assert!(request.to_lowercase().contains("token: t0k"), "{request}");
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
}
