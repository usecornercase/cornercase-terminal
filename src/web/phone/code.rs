use std::fmt::Write as _;
use std::net::SocketAddr;

use crate::text::percent_encoded;

const NAME_CHARS: usize = 64;

pub(crate) fn machine_code(routes: &[SocketAddr], code: &str, name: Option<&str>) -> Option<String> {
    let (first, rest) = routes.split_first()?;
    let mut url = format!("ws://{first}/?code={code}");
    for route in rest {
        let _ = write!(url, "&also={}", route.ip());
    }
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        let cut: String = name.chars().take(NAME_CHARS).collect();
        let _ = write!(url, "&name={}", percent_encoded(&cut));
    }
    Some(url)
}

#[cfg(test)]
mod tests;
