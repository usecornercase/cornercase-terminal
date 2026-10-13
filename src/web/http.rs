use std::fmt::Write as _;
use std::io::{self, Read, Write};

const MAX_HEAD: usize = 16 * 1024;
const MAX_HEADERS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Request {
    pub(crate) method: String,
    pub(crate) path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

impl Request {
    pub(super) fn parse(head: &[u8]) -> Option<Self> {
        let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
        let mut request = httparse::Request::new(&mut headers);
        if !request.parse(head).ok()?.is_complete() {
            return None;
        }
        let (path, query) = request.path?.split_once('?').unwrap_or((request.path?, ""));
        let query = query
            .split('&')
            .filter_map(|pair| pair.split_once('=').or(Some((pair, ""))))
            .filter(|(key, _)| !key.is_empty())
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let headers = request
            .headers
            .iter()
            .map(|h| (h.name.to_ascii_lowercase(), String::from_utf8_lossy(h.value).trim().to_string()))
            .collect();
        Some(Self { method: request.method?.to_string(), path: path.to_string(), query, headers })
    }

    pub(super) fn read(stream: &mut impl Read) -> io::Result<Option<Self>> {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if head.len() >= MAX_HEAD || stream.read(&mut byte)? == 0 {
                return Ok(None);
            }
            head.push(byte[0]);
        }
        Ok(Self::parse(&head))
    }

    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub(super) fn query(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub(super) fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .filter(|(key, _)| key == "cookie")
            .flat_map(|(_, value)| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }

    pub(super) fn same_origin(&self) -> bool {
        let Some(origin) = self.header("origin") else { return false };
        let Some((scheme, host)) = origin.split_once("://") else { return false };
        if !matches!(scheme, "http" | "https") {
            return false;
        }
        [self.header("x-forwarded-host"), self.header("host")].into_iter().flatten().any(|h| h == host)
    }

    pub(super) fn forwarded(&self) -> bool {
        ["x-forwarded-for", "x-forwarded-host", "tailscale-headers-info"].iter().any(|h| self.header(h).is_some())
    }

    pub(super) fn wants_websocket(&self) -> bool {
        self.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"))
    }
}

pub(super) struct Response {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Response {
    pub(super) fn new(status: &'static str, kind: &'static str, body: impl Into<Vec<u8>>) -> Self {
        let headers = vec![
            ("Content-Type", kind.into()),
            ("Cache-Control", "no-store".into()),
            ("X-Content-Type-Options", "nosniff".into()),
            ("X-Frame-Options", "DENY".into()),
            ("Referrer-Policy", "no-referrer".into()),
        ];
        Self { status, headers, body: body.into() }
    }

    pub(super) fn html(status: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self::new(status, "text/html; charset=utf-8", body).with(
            "Content-Security-Policy",
            "default-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' wss: ws:; frame-ancestors 'none'"
                .into(),
        )
    }

    pub(super) fn redirect(to: &str) -> Self {
        Self::new("303 See Other", "text/plain; charset=utf-8", "").with("Location", to.into())
    }

    pub(super) fn with(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }

    pub(super) fn send(&self, out: &mut impl Write) -> io::Result<()> {
        let mut head =
            format!("HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n", self.status, self.body.len());
        for (name, value) in &self.headers {
            let _ = write!(head, "{name}: {value}\r\n");
        }
        head.push_str("\r\n");
        out.write_all(head.as_bytes())?;
        out.write_all(&self.body)?;
        out.flush()
    }
}

#[cfg(test)]
mod tests;
