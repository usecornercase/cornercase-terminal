use std::time::Duration;

use serde_json::Value;
use ureq::http::Response;
use ureq::typestate::WithBody;
use ureq::{Agent, RequestBuilder};

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
const REDIRECTS: u32 = 10;
const STRICT_REDIRECTS: u32 = 3;

pub(crate) struct Service {
    pub(crate) name: &'static str,
    pub(crate) rejected: &'static str,
}

pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) json: Value,
}

impl Answer {
    pub(crate) fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

fn agent(timeout: Duration) -> Agent {
    following(timeout, REDIRECTS)
}

fn following(timeout: Duration, redirects: u32) -> Agent {
    Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(redirects)
        .build()
        .into()
}

pub(crate) fn get(service: &Service, url: &str, query: &[(&str, &str)], headers: &[(&str, &str)]) -> Result<Answer> {
    get_with(&agent(TIMEOUT), service, url, query, headers)
}

pub(crate) fn get_without_redirects(
    service: &Service,
    url: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
) -> Result<Answer> {
    let answer = get_with(&following(TIMEOUT, 0), service, url, query, headers)?;
    if (300..400).contains(&answer.status) {
        return Err(Error::Api(format!(
            "{} answered {} with a redirect, which is not followed so the token is not sent elsewhere: use the \
             address it leads to",
            service.name, answer.status
        )));
    }
    Ok(answer)
}

fn get_with(
    agent: &Agent,
    service: &Service,
    url: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
) -> Result<Answer> {
    let mut request = agent.get(url).header("Accept", "application/json");
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    finish(service, request.query_pairs(query.iter().copied()).call())
}

pub(crate) fn post(service: &Service, url: &str, headers: &[(&str, &str)], body: &Value) -> Result<Answer> {
    let mut request = agent(TIMEOUT).post(url).header("Accept", "application/json");
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    finish(service, request.content_type("application/json").send(body.to_string()))
}

pub(crate) fn post_within(service: &Service, url: &str, body: &Value, timeout: Duration) -> Result<Answer> {
    let request = agent(timeout).post(url).header("Accept", "application/json").content_type("application/json");
    let mut response = reached(service, request.send(body.to_string()), timeout)?;
    read(service, &mut response)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
    Patch,
    Put,
    Delete,
}

pub(crate) fn call_within(
    service: &Service,
    method: Method,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Answer> {
    let agent = agent(timeout);
    let result = match method {
        Method::Get => headed(agent.get(url), headers).call(),
        Method::Post => carrying(headed(agent.post(url), headers), body),
        Method::Patch => carrying(headed(agent.patch(url), headers), body),
        Method::Put => carrying(headed(agent.put(url), headers), body),
        Method::Delete => headed(agent.delete(url), headers).call(),
    };
    let mut response = reached(service, result, timeout)?;
    read(service, &mut response)
}

fn headed<B>(mut request: RequestBuilder<B>, headers: &[(&str, &str)]) -> RequestBuilder<B> {
    request = request.header("Accept", "application/json");
    for (key, value) in headers {
        request = request.header(*key, *value);
    }
    request
}

fn carrying(
    request: RequestBuilder<WithBody>,
    body: Option<&Value>,
) -> std::result::Result<Response<ureq::Body>, ureq::Error> {
    match body {
        Some(body) => request.content_type("application/json").send(body.to_string()),
        None => request.send_empty(),
    }
}

pub(crate) fn download(service: &Service, url: &str) -> Result<Vec<u8>> {
    download_within(service, url, DOWNLOAD_LIMIT, DOWNLOAD_TIMEOUT)
}

pub(crate) const TOO_LARGE: &str = "the answer is larger than its limit";

pub(crate) fn download_within(service: &Service, url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
    body(service, &agent(timeout), url, limit, timeout)
}

pub(crate) fn download_strictly(service: &Service, url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .max_redirects(STRICT_REDIRECTS)
        .https_only(url.starts_with("https://"))
        .build()
        .into();
    body(service, &agent, url, limit, timeout)
}

fn body(service: &Service, agent: &Agent, url: &str, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
    let mut response = reached(service, agent.get(url).call(), timeout)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(Error::Api(format!("{} answered {status} for {url}", service.name)));
    }
    response.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| match e {
        ureq::Error::BodyExceedsLimit(_) => Error::Api(TOO_LARGE.into()),
        e => Error::Api(format!("could not download from {}: {e}", service.name)),
    })
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
    read(service, &mut response)
}

fn read(service: &Service, response: &mut Response<ureq::Body>) -> Result<Answer> {
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Api(format!("could not read the answer of {}: {e}", service.name)))?;
    let json = serde_json::from_str(&text).unwrap_or(Value::Null);
    Ok(Answer { status, json })
}

pub(crate) fn into_json(answer: Answer, missing: &str, failure: impl Fn(&Answer) -> Error) -> Result<Value> {
    if answer.status == 404 {
        return Err(Error::Api(missing.into()));
    }
    if !answer.ok() {
        return Err(failure(&answer));
    }
    Ok(answer.json)
}

pub(crate) fn origin(url: &str) -> &str {
    let after_scheme = url.find("://").map_or(0, |i| i + 3);
    url[after_scheme..].find('/').map_or(url, |i| &url[..after_scheme + i])
}

pub(crate) fn text(value: &Value, pointer: &str) -> String {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_string()
}

pub(crate) fn path_segment(key: &str) -> String {
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
mod tests;
