use std::fmt::Write;
use std::path::Path;

pub const CRUMB_SEPARATOR: &str = " › ";

pub fn folder_name(path: &Path, home: Option<&Path>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub fn truncate_right(s: &str, max: usize) -> String {
    if s.chars().count() <= max || max < 2 {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{}…", head.trim_end_matches('…'))
}

pub fn truncate_left(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max || max < 2 {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    format!("…{}", tail.trim_start_matches('…'))
}

pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests;
