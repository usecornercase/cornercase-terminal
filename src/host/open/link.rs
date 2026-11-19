use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Link {
    pub(crate) url: String,
}

pub(crate) const MAX_LINK: usize = 2048;

const SCHEMES: [&str; 2] = ["http://", "https://"];
const NOT_IN_A_URL: &[u8] = b"\"<>\\^`{|}";

pub(crate) fn host(url: &str) -> Result<&str, &'static str> {
    if url.len() > MAX_LINK {
        return Err("it is too long");
    }
    let rest =
        SCHEMES.iter().find_map(|scheme| url.strip_prefix(scheme)).ok_or("it is not an http or https address")?;
    if !url.bytes().all(|byte| byte.is_ascii_graphic() && !NOT_IN_A_URL.contains(&byte)) {
        return Err("it holds a character a web address cannot have");
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return Err("it carries a user name");
    }
    let host = without_port(authority);
    if bracketed(host) || named(host) { Ok(host) } else { Err("it has no host") }
}

fn without_port(authority: &str) -> &str {
    match authority.rsplit_once(':') {
        Some((host, port))
            if (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit()) && !host.is_empty() =>
        {
            host
        }
        _ => authority,
    }
}

fn bracketed(host: &str) -> bool {
    host.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')).is_some_and(|inner| {
        !inner.is_empty() && inner.bytes().all(|byte| byte.is_ascii_hexdigit() || b":.".contains(&byte))
    })
}

fn named(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.'])
        && host.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-".contains(&byte))
}

#[cfg(test)]
mod tests;
